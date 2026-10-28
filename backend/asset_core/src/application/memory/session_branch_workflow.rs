use crate::backend::application::memory::recent::{
    RecentConversationSessionListParams, RecentConversationView,
};
use crate::backend::application::{AppError, AppResult, AppService, ConversationOutlineParams};
use crate::backend::domain::conversations::outline::ConversationOutlineDetailLevel;
use crate::backend::domain::conversations::ConversationOutline;
use crate::backend::infrastructure::memory::storage::MemoryStorage;
use crate::backend::store::memory::watermark_repo::{
    load_watermarks_sqlx, upsert_watermark_sqlx, MemorySessionWatermarkRecord,
};

#[derive(Debug, Clone, Default)]
pub struct SessionBranchWorkflowReport {
    pub scanned_count: usize,
    pub processed_count: usize,
    pub skipped_count: usize,
}

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
        extractor: Option<&dyn SessionBranchExtractorAgent>,
        storage_override: Option<&MemoryStorage>,
    ) -> AppResult<SessionBranchWorkflowReport> {
        self.run_session_branch_workflow_at(extractor, storage_override, chrono::Utc::now())
            .await
    }

    pub async fn run_session_branch_workflow_at(
        &self,
        extractor: Option<&dyn SessionBranchExtractorAgent>,
        storage_override: Option<&MemoryStorage>,
        now: chrono::DateTime<chrono::Utc>,
    ) -> AppResult<SessionBranchWorkflowReport> {
        let pool = self.pool();
        let tenant_id = self.tenant_id();

        let owned_storage;
        let storage = match storage_override {
            Some(s) => s,
            None => {
                owned_storage = MemoryStorage::new(tenant_id);
                &owned_storage
            }
        };

        let watermarks = load_watermarks_sqlx(pool, tenant_id).await?;
        let recent_sessions = self
            .list_recent_conversation_sessions_at(
                RecentConversationSessionListParams {
                    view: RecentConversationView::Time,
                    limit: Some(1000),
                    offset: Some(0),
                },
                now,
            )
            .await?;

        let mut report = SessionBranchWorkflowReport {
            scanned_count: recent_sessions.len(),
            processed_count: 0,
            skipped_count: 0,
        };

        let extractor = match extractor {
            Some(e) => e,
            None => {
                // 无提取器注入时，止血关停机械复读，不向磁盘写入任何会话产物
                report.skipped_count = recent_sessions.len();
                return Ok(report);
            }
        };

        for item in recent_sessions {
            let session = item.session;
            let session_updated_at = session.updated_at.clone().unwrap_or_default();
            let short_id = crate::backend::domain::conversation_id_fragment(&session.id);

            // 水位比对：若 updated_at 一致且磁盘摘要文件存在，直接跳过（0 LLM 调用）
            if let Some(wm) = watermarks.get(&session.id) {
                if wm.source_updated_at == session_updated_at && !session_updated_at.is_empty() {
                    let disk_path = storage.root_dir().join(&wm.summary_rel_path);
                    if disk_path.exists() {
                        report.skipped_count += 1;
                        continue;
                    }
                }
            }

            // 拉取 --qa 问答主干
            let outline = self
                .get_conversation_outline(ConversationOutlineParams {
                    id: session.id.clone(),
                    detail_level: ConversationOutlineDetailLevel::Qa,
                })
                .await?;

            let mut qa_turns = Vec::new();
            if let ConversationOutline::Session(s) = outline {
                for row in s.turns.rows {
                    if let crate::backend::domain::conversations::outline::SessionTurnRow::Qa {
                        index,
                        turn_id,
                        question,
                        answer,
                    } = row
                    {
                        qa_turns.push((index, turn_id, question, answer));
                    }
                }
            }

            if qa_turns.is_empty() {
                upsert_watermark_sqlx(
                    pool,
                    &MemorySessionWatermarkRecord {
                        tenant_id: tenant_id.to_string(),
                        session_id: session.id.clone(),
                        project_path: item.project_path.clone(),
                        source_updated_at: session_updated_at,
                        summary_rel_path: String::new(),
                        has_signal: false,
                        updated_at: chrono::Utc::now().to_rfc3339(),
                    },
                )
                .await?;
                report.skipped_count += 1;
                continue;
            }

            let extracted = extractor
                .extract_branches(
                    &session.id,
                    &short_id,
                    item.project_path.as_deref(),
                    &qa_turns,
                )
                .await?;

            let date_str = session
                .started_at
                .as_deref()
                .or(session.updated_at.as_deref())
                .and_then(|ts| ts.split('T').next())
                .unwrap_or("unknown");

            let rel_path = format!("sessions/{}_{}.md", date_str, short_id);

            let mut md = String::new();
            md.push_str("---\n");
            md.push_str(&format!("session_id: {}\n", session.id));
            if let Some(ref proj) = item.project_path {
                md.push_str(&format!("project_path: {}\n", proj));
            }
            md.push_str(&format!("updated_at: {}\n", session_updated_at));
            md.push_str("---\n\n");
            md.push_str(&format!("# 会话 {} 思维分叉与任务进展\n\n", short_id));

            md.push_str("## 🍂 待探索的悬空叶子节点 (Unexplored Branches)\n");
            if extracted.unexplored_branches.is_empty() {
                md.push_str("- 无待探索分支\n");
            } else {
                for branch in &extracted.unexplored_branches {
                    md.push_str(&format!(
                        "- [{}#T{}](aiwc://session/{}/turn/{}): {}\n",
                        short_id, branch.turn_index, short_id, branch.turn_index, branch.detail
                    ));
                }
            }
            md.push('\n');

            md.push_str("## ✅ 已完成的闭环任务 (Completed Tasks)\n");
            if extracted.completed_tasks.is_empty() {
                md.push_str("- 无已完成任务\n");
            } else {
                for task in &extracted.completed_tasks {
                    md.push_str(&format!(
                        "- [{}#T{}](aiwc://session/{}/turn/{}): {}\n",
                        short_id, task.turn_index, short_id, task.turn_index, task.detail
                    ));
                }
            }

            storage
                .write_markdown_atomic(&rel_path, &md)
                .map_err(AppError::from)?;

            upsert_watermark_sqlx(
                pool,
                &MemorySessionWatermarkRecord {
                    tenant_id: tenant_id.to_string(),
                    session_id: session.id.clone(),
                    project_path: item.project_path.clone(),
                    source_updated_at: session_updated_at,
                    summary_rel_path: rel_path,
                    has_signal: extracted.has_signal,
                    updated_at: chrono::Utc::now().to_rfc3339(),
                },
            )
            .await?;

            report.processed_count += 1;
        }

        Ok(report)
    }
}

#[cfg(test)]
#[path = "session_branch_workflow_tests.rs"]
mod tests;
