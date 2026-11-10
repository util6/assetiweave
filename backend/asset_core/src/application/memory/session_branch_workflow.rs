use crate::backend::application::memory::project_path_to_slug;
use crate::backend::application::memory::reasoning_loop::{
    execute_inspect_commit_loop, format_topic_tree_summary,
};
use crate::backend::application::memory::recent::{
    RecentConversationSessionListParams, RecentConversationView,
};
use crate::backend::application::{AppError, AppResult, AppService, ConversationOutlineParams};
use crate::backend::domain::agents::{ActionId, AgentId};
use crate::backend::domain::conversations::outline::ConversationOutlineDetailLevel;
use crate::backend::domain::conversations::ConversationOutline;
use crate::backend::infrastructure::agent_execution::AgentExecutionRuntime;
use crate::backend::infrastructure::memory::storage::MemoryStorage;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SessionBranchWorkflowReport {
    pub scanned_count: usize,
    pub processed_count: usize,
    pub skipped_count: usize,
}

/// 回溯扫描的会话上限。按最后活动时间倒序取最新的一批，
/// 既覆盖全部近期活跃会话，也覆盖较早但仍有未处理轮次的积压会话。
///
/// 注意：该值必须 ≤ `list_recent_conversation_sessions` 的 `limit` 上限（2000），
/// 否则请求会被静默截断，回溯深度与直觉不符。
const BACKLOG_SCAN_LIMIT: usize = 2000;

#[derive(Debug, Clone, Default)]
pub struct ExtractedSessionBranches {
    pub unexplored_branches: Vec<BranchItem>,
    pub completed_tasks: Vec<BranchItem>,
    pub has_signal: bool,
}

#[derive(Debug, Clone)]
pub struct BranchItem {
    pub turn_index: usize,
    pub short_id: String,
    pub title: String,
    pub detail: String,
}

#[async_trait::async_trait]
pub trait SessionBranchExtractorAgent: Send + Sync {
    async fn extract_branches(
        &self,
        session_id: &str,
        short_id: &str,
        project_path: Option<&str>,
        qa_turns: &[(usize, String, String, Option<String>)],
    ) -> AppResult<ExtractedSessionBranches>;
}

impl AppService {
    pub async fn run_session_branch_workflow(
        &self,
        runtime_override: Option<&dyn AgentExecutionRuntime>,
        storage_override: Option<&MemoryStorage>,
    ) -> AppResult<SessionBranchWorkflowReport> {
        self.run_session_branch_workflow_at(runtime_override, storage_override, chrono::Utc::now())
            .await
    }

    pub async fn run_session_branch_workflow_at(
        &self,
        runtime_override: Option<&dyn AgentExecutionRuntime>,
        storage_override: Option<&MemoryStorage>,
        now: chrono::DateTime<chrono::Utc>,
    ) -> AppResult<SessionBranchWorkflowReport> {
        let owned_storage;
        let storage: &MemoryStorage = match storage_override {
            Some(s) => s,
            None => {
                owned_storage = self.memory_storage();
                &owned_storage
            }
        };
        // 第一阶段默认按活动时间向后兜底回溯全部历史会话，不做窄窗口裁剪：
        // 水位仅为 0（从未提炼）或水位落后的积压会话即使早已滑出近期窗口，
        // 也必须被重新发现，否则历史未闭环任务会永久漏扫。
        let mut recent_sessions = self
            .list_recent_conversation_sessions_at(
                RecentConversationSessionListParams {
                    view: RecentConversationView::Time,
                    window_hours: Some(0),
                    limit: Some(BACKLOG_SCAN_LIMIT),
                    offset: Some(0),
                },
                now,
            )
            .await?;

        // Oldest-First (升序排序：早发生的会话先演化分支，后发生的会话在其上继续演化或闭环)
        recent_sessions.sort_by(|left, right| {
            left.last_activity_at
                .cmp(&right.last_activity_at)
                .then_with(|| left.session.id.cmp(&right.session.id))
        });

        let mut report = SessionBranchWorkflowReport {
            scanned_count: recent_sessions.len(),
            processed_count: 0,
            skipped_count: 0,
        };

        // 确定执行运行时与 AgentId / Model
        let (runtime, agent_id, model) = match runtime_override {
            Some(rt) => {
                let resolved =
                    self.resolve_available_agent_for(&ActionId::new("memory.generation"));
                let (agent_id, model) = match resolved {
                    Ok((a, m)) => (a, m),
                    Err(_) => (
                        AgentId::parse("memory-generation").expect("static agent id is valid"),
                        None,
                    ),
                };
                (rt, agent_id, model)
            }
            None => {
                let resolved =
                    self.resolve_available_agent_for(&ActionId::new("memory.generation"));
                match resolved {
                    Ok((agent_id, model)) => (self.agent_runtime.as_ref(), agent_id, model),
                    Err(_) => {
                        // 无可用认知 Agent，止血关停，不向磁盘写入任何产物
                        report.skipped_count = recent_sessions.len();
                        return Ok(report);
                    }
                }
            }
        };

        for item in recent_sessions {
            let session = item.session;
            let short_id = crate::backend::domain::conversation_id_fragment(&session.id);
            let project_slug = project_path_to_slug(item.project_path.as_deref().unwrap_or(""));

            // 低成本水位短路：先用列表里的 turn_count 与已记录水位比对，
            // 已处理完的会话无需再拉大纲、无需进入认知提炼。
            // 这是全量回溯发现（可能上千条历史会话）不拖慢流水线的前提。
            let known_watermark = self
                .check_memory_session_watermark_in(&project_slug, &session.id, storage)
                .await?;
            if let Some(wm) = known_watermark {
                if item.turn_count > 0 && wm as usize >= item.turn_count {
                    report.skipped_count += 1;
                    continue;
                }
            }

            // 拉取 --qa 问答主干
            let outline = self
                .get_conversation_outline(ConversationOutlineParams {
                    id: session.id.clone(),
                    detail_level: ConversationOutlineDetailLevel::Qa,
                })
                .await?;

            let mut turns_outline = Vec::new();
            let mut qa_map = std::collections::HashMap::new();
            let mut max_turn = 0u32;

            if let ConversationOutline::Session(s) = outline {
                for row in s.turns.rows {
                    if let crate::backend::domain::conversations::outline::SessionTurnRow::Qa {
                        index,
                        question,
                        answer,
                        ..
                    } = row
                    {
                        let idx_u32 = index as u32;
                        if idx_u32 > max_turn {
                            max_turn = idx_u32;
                        }
                        let ans_str = answer.unwrap_or_default();
                        turns_outline.push((index, question.clone(), ans_str.clone()));
                        qa_map.insert(idx_u32, (question, ans_str));
                    }
                }
            }

            // 水位短路比对（0 LLM 调用）：
            // 若 .state/projects/<slug>.json 中的水位已到达或超过该会话在问答主干里的
            // 最大轮次（`max_turn`），直接跳过。上面基于 `turn_count` 的预筛是廉价粗筛，
            // 这里才是与真实问答轮次对齐的精确判定。
            if let Some(wm) = known_watermark {
                if wm >= max_turn && max_turn > 0 {
                    report.skipped_count += 1;
                    continue;
                }
            }

            if turns_outline.is_empty() {
                self.advance_memory_session_watermark_in(&project_slug, &session.id, 0, storage)
                    .await?;
                report.skipped_count += 1;
                continue;
            }

            // 读取既有主题树概要注入 Prompt 上下文
            let project_state = self
                .get_project_memory_state_in(&project_slug, storage)
                .await?;
            let existing_summary = project_state
                .as_ref()
                .map(|s| format_topic_tree_summary(&s.topic_trees))
                .unwrap_or_default();

            // 执行 2 步轻量 Agent 循环（含 JSON 脱壳与单次重试）
            let loop_res = execute_inspect_commit_loop(
                runtime,
                &agent_id,
                model.as_deref(),
                &session.id,
                &short_id,
                &project_slug,
                max_turn,
                &turns_outline,
                &existing_summary,
                |turn| qa_map.get(&turn).cloned(),
            )
            .await;

            match loop_res {
                Ok(payload) => {
                    // 提交主题演进（自动维护多叉树并原子写入 .state/ 与 Markdown 投影）
                    self.commit_memory_topic_evolution_to(payload, storage)
                        .await?;
                    self.advance_memory_session_watermark_in(
                        &project_slug,
                        &session.id,
                        max_turn,
                        storage,
                    )
                    .await?;
                    report.processed_count += 1;
                }
                Err(err) => {
                    tracing::warn!(
                        session_id = %session.id,
                        error = %err,
                        "Session cognitive extraction failed; skipping session to ensure pipeline resilience"
                    );
                    report.skipped_count += 1;
                }
            }
        }

        Ok(report)
    }
}

#[cfg(test)]
#[path = "session_branch_workflow_tests.rs"]
mod tests;
