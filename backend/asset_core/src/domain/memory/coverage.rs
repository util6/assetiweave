use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};

use super::{CandidateSession, SessionMemory, SessionMemorySourceReference, SessionMemoryStatus};

/// 会话覆盖状态（覆盖状态机三态模型）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SessionCoverageState {
    /// 绿色：事实已就绪（active session memory + canonical references 有有效定位符）
    Ready,
    /// 琥珀色：瞬态（等待调度、正在执行、或租约超时待重投）
    Transient,
    /// 红色：已隔离（业务重试超限、物理调度超限、或配置人工排除）
    Quarantined,
}

/// 隔离原因分类
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QuarantineReason {
    /// 用户配置手动排除 (excluded_session_ids / excluded_source_ids)
    ManuallyExcluded,
    /// 业务重试耗尽或物理调度上限熔断
    PoisonOrLimitExceeded,
    /// 明确的致命错误 (例如 agent 不存在等)
    TerminalFailure(String),
}

/// 候选会话的单条覆盖分析视图
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CandidateSessionCoverage {
    pub session_id: String,
    pub project_key: String,
    pub title: String,
    pub state: SessionCoverageState,
    pub quarantine_reason: Option<QuarantineReason>,
    pub job_status: Option<String>,
    pub last_error: Option<String>,
}

/// 候选会话作业状态上下文简报（用于纯内存投影计算）
#[derive(Debug, Clone, Default)]
pub struct SessionJobEvaluationContext {
    pub status: String,
    pub attempt_count: i64,
    pub retry_count: i64,
    pub last_error: Option<String>,
}

/// 纯查询会话覆盖投影（Pure Query Coverage Projection）
/// 严格遵守红线 2：零新增存储表，随用随算，以 SQLite 现有表为唯一真相源。
#[derive(Debug, Clone, Default)]
pub struct SessionCoverageProjection {
    pub ready_sessions: Vec<CandidateSession>,
    pub transient_sessions: Vec<CandidateSession>,
    pub quarantined_sessions: Vec<(CandidateSession, QuarantineReason)>,
}

impl SessionCoverageProjection {
    /// 根据候选会话列表、active 会话记忆、引用表及作业上下文纯函数计算覆盖投影
    pub fn evaluate(
        candidates: Vec<CandidateSession>,
        active_memories: &BTreeMap<String, SessionMemory>,
        source_references: &BTreeMap<String, Vec<SessionMemorySourceReference>>,
        jobs: &HashMap<String, SessionJobEvaluationContext>,
        excluded_session_ids: &[String],
        excluded_source_ids: &[String],
    ) -> Self {
        let excluded_sessions: HashSet<&str> =
            excluded_session_ids.iter().map(String::as_str).collect();
        let excluded_sources: HashSet<&str> =
            excluded_source_ids.iter().map(String::as_str).collect();

        let mut ready_sessions = Vec::new();
        let mut transient_sessions = Vec::new();
        let mut quarantined_sessions = Vec::new();

        for candidate in candidates {
            // 1. 人工配置排除 -> Quarantined
            if excluded_sessions.contains(candidate.session_id.as_str())
                || excluded_sources.contains(candidate.source_id.as_str())
            {
                quarantined_sessions.push((candidate, QuarantineReason::ManuallyExcluded));
                continue;
            }

            // 2. 检查事实与引用有效性 -> 若就绪则为 Ready
            let has_active_mem = active_memories.contains_key(&candidate.session_id);
            let has_valid_refs = source_references
                .get(&candidate.session_id)
                .is_some_and(|refs| {
                    refs.iter().any(|r| {
                        r.question_id.is_some()
                            || r.turn_id.is_some()
                            || r.part_id.is_some()
                            || r.node_id.is_some()
                    })
                });

            if has_active_mem && has_valid_refs {
                ready_sessions.push(candidate);
                continue;
            }

            // 3. 无就绪事实，依据 Job 状态判定瞬态还是隔离
            match jobs.get(&candidate.session_id) {
                Some(job) => {
                    let status = job.status.as_str();
                    if matches!(status, "queued" | "running") {
                        transient_sessions.push(candidate);
                    } else if matches!(status, "failed" | "canceled" | "skipped") {
                        // 守住红线 5：若属于未超限的租约超时，视为瞬态可重投；真正超限或明确业务失败才隔离
                        if job.last_error.as_deref() == Some("lease_expired")
                            && job.attempt_count < 10
                        {
                            transient_sessions.push(candidate);
                        } else {
                            let reason = if let Some(ref err) = job.last_error {
                                if err == "lease_expired_limit_exceeded"
                                    || job.retry_count >= 5
                                    || job.attempt_count >= 10
                                {
                                    QuarantineReason::PoisonOrLimitExceeded
                                } else {
                                    QuarantineReason::TerminalFailure(err.clone())
                                }
                            } else {
                                QuarantineReason::PoisonOrLimitExceeded
                            };
                            quarantined_sessions.push((candidate, reason));
                        }
                    } else {
                        // 其它未知状态保守归入瞬态待调度
                        transient_sessions.push(candidate);
                    }
                }
                None => {
                    // 尚未入队过 Job，属于待阶段 1 插入的瞬态
                    transient_sessions.push(candidate);
                }
            }
        }

        Self {
            ready_sessions,
            transient_sessions,
            quarantined_sessions,
        }
    }

    /// 当前窗口内的瞬态会话是否已全部收敛（用于阶段 2 门禁前置判定）
    pub fn is_transient_empty(&self) -> bool {
        self.transient_sessions.is_empty()
    }

    /// 获取所有就绪会话 ID 集合
    pub fn ready_session_ids(&self) -> HashSet<String> {
        self.ready_sessions
            .iter()
            .map(|s| s.session_id.clone())
            .collect()
    }

    /// 获取所有被隔离会话 ID 与原因列表
    pub fn quarantined_summary(&self) -> Vec<(String, String)> {
        self.quarantined_sessions
            .iter()
            .map(|(s, r)| (s.session_id.clone(), format!("{:?}", r)))
            .collect()
    }
}

#[cfg(test)]
#[path = "coverage_tests.rs"]
mod tests;
