//! 记忆引用一致性审计：报告三张持久化了「问题标识」的引用表里哪些指向不存在的分组，
//! 并把孤儿重新指向其所在会话内最近的有效分组；无法判定时显式标记失效而非删除。
//!
//! 五项计数构成完整划分：`total = valid + orphaned + repaired + manual_review`。

use crate::backend::application::prelude::*;

mod apply;
mod resolve;
mod scan;

pub const MEMORY_REFERENCE_AUDIT_SCHEMA_VERSION: u32 = 1;

pub(super) const REMAPPED: &str = "remapped";
pub(super) const INVALIDATED: &str = "invalidated";

pub(super) const STRATEGY_REDIRECT: &str = "redirect_tombstone";
pub(super) const STRATEGY_TURN_MEMBERSHIP: &str = "turn_membership";
pub(super) const STRATEGY_NEAREST_GROUP: &str = "nearest_group";
pub(super) const STRATEGY_SINGLE_GROUP: &str = "single_session_group";
pub(super) const STRATEGY_NO_GROUP: &str = "session_group_missing";
pub(super) const STRATEGY_AMBIGUOUS: &str = "ambiguous_group";

/// 五项计数构成对「持久化了问题标识的引用」的完整划分：
/// `total = valid + orphaned + repaired + manual_review`。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct MemoryReferenceAuditCounts {
    /// 三张引用表里 `question_id` 非空的行数合计。
    pub total_references: i64,
    /// 问题标识当前可在同会话内解析，且从未被本审计修复过。
    pub valid: i64,
    /// 问题标识当前不可解析，且尚未落账判定。
    pub orphaned: i64,
    /// 已被本审计重新指向到有效分组，并仍指向该分组。
    pub repaired: i64,
    /// 无法自动判定、已被显式标记失效，等待人工处理。
    pub manual_review: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MemoryReferenceFinding {
    pub reference_table: String,
    pub reference_id: String,
    pub record_kind: String,
    pub session_id: String,
    pub question_id: String,
    pub turn_id: Option<String>,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MemoryReferenceRepair {
    pub reference_table: String,
    pub reference_id: String,
    pub previous_question_id: String,
    pub resolved_question_id: String,
    pub strategy: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MemoryReferenceAuditReport {
    pub schema_version: u32,
    pub dry_run: bool,
    pub generated_at: String,
    pub counts: MemoryReferenceAuditCounts,
    /// 本次运行真正写入的重新指向；dry-run 时恒为空。
    pub repairs: Vec<MemoryReferenceRepair>,
    /// 本次运行计算出的重新指向计划；每次运行都可重现。
    pub planned_repairs: Vec<MemoryReferenceRepair>,
    /// 本次运行计算出的失效计划；dry-run 时据此预告将标记失效的引用。
    pub planned_invalidations: Vec<MemoryReferenceFinding>,
    /// 需人工判定清单：已显式标记失效的引用。
    pub manual_review: Vec<MemoryReferenceFinding>,
}

impl AppService {
    /// 只读一致性审计：输出五项计数、可重现的修复计划与需人工判定清单，不写库。
    pub async fn audit_memory_references(&self) -> AppResult<MemoryReferenceAuditReport> {
        let snapshot = scan::compute_audit(self.pool(), self.tenant_id()).await?;
        Ok(report_from(snapshot, true, Vec::new()))
    }

    /// 修复入口。`dry_run` 只回放审计与修复计划；写入必须显式 `yes = true`。
    pub async fn repair_memory_references(
        &self,
        params: MemoryReferenceRepairParams,
    ) -> AppResult<MemoryReferenceAuditReport> {
        let pool = self.pool();
        let tenant_id = self.tenant_id();
        if params.dry_run {
            let snapshot = scan::compute_audit(pool, tenant_id).await?;
            return Ok(report_from(snapshot, true, Vec::new()));
        }
        if !params.yes {
            return Err(AppError::Validation(
                "memory.reference.repair requires yes=true".to_string(),
            ));
        }
        let snapshot = scan::compute_audit(pool, tenant_id).await?;
        let now = Utc::now().to_rfc3339();
        let mut tx = self.db.begin_immediate().await?;
        let applied = apply::apply_snapshot(&mut tx, tenant_id, &snapshot, &now).await?;
        tx.commit().await?;
        let post = scan::compute_audit(pool, tenant_id).await?;
        Ok(report_from(post, false, applied)
            .with_plan(snapshot.planned_repairs, snapshot.planned_invalidations))
    }
}

fn report_from(
    snapshot: scan::AuditSnapshot,
    dry_run: bool,
    repairs: Vec<MemoryReferenceRepair>,
) -> MemoryReferenceAuditReport {
    MemoryReferenceAuditReport {
        schema_version: MEMORY_REFERENCE_AUDIT_SCHEMA_VERSION,
        dry_run,
        generated_at: Utc::now().to_rfc3339(),
        counts: snapshot.counts,
        repairs,
        planned_repairs: snapshot
            .planned_repairs
            .into_iter()
            .map(|planned| planned.repair)
            .collect(),
        planned_invalidations: snapshot
            .planned_invalidations
            .into_iter()
            .map(|planned| planned.finding)
            .collect(),
        manual_review: snapshot.manual_review,
    }
}

impl MemoryReferenceAuditReport {
    /// 写入响应里保留写入前算出的 dry-run 计划，确保「先有 dry-run 输出再写入」可追溯。
    fn with_plan(
        mut self,
        planned_repairs: Vec<scan::PlannedRepair>,
        planned_invalidations: Vec<scan::PlannedInvalidation>,
    ) -> Self {
        self.planned_repairs = planned_repairs
            .into_iter()
            .map(|planned| planned.repair)
            .collect();
        self.planned_invalidations = planned_invalidations
            .into_iter()
            .map(|planned| planned.finding)
            .collect();
        self
    }
}

#[cfg(test)]
#[path = "../reference_audit_tests.rs"]
mod tests;
