//! 只读的一致性扫描：把三张引用表里「持久化了问题标识」的行分类，
//! 并为每个孤儿计算可重放的修复计划。

use crate::backend::application::prelude::*;
use sqlx::{AssertSqlSafe, SqlitePool};

use super::resolve::{resolve_orphan, Resolution};
use super::{
    MemoryReferenceAuditCounts, MemoryReferenceFinding, MemoryReferenceRepair, INVALIDATED,
    REMAPPED,
};

/// 持久化了「问题标识」的三张引用表。三表的问题定位列同名，因此共用一套审计语句。
const REFERENCE_TABLES: [&str; 3] = [
    "memory_evidence_snapshots",
    "session_memory_source_references",
    "memory_item_source_references",
];

#[derive(Debug, Clone)]
pub(super) struct ReferenceRow {
    pub(super) table: &'static str,
    pub(super) id: String,
    pub(super) record_kind: String,
    pub(super) session_id: String,
    pub(super) question_id: String,
    pub(super) turn_id: Option<String>,
}

impl ReferenceRow {
    pub(super) fn key(&self) -> (String, String) {
        (self.table.to_string(), self.id.clone())
    }

    pub(super) fn finding(&self, reason: &str) -> MemoryReferenceFinding {
        MemoryReferenceFinding {
            reference_table: self.table.to_string(),
            reference_id: self.id.clone(),
            record_kind: self.record_kind.clone(),
            session_id: self.session_id.clone(),
            question_id: self.question_id.clone(),
            turn_id: self.turn_id.clone(),
            reason: reason.to_string(),
        }
    }
}

#[derive(Debug, Clone)]
pub(super) struct LedgerRow {
    pub(super) reference_table: String,
    pub(super) reference_id: String,
    pub(super) previous_question_id: String,
    pub(super) resolution: String,
    pub(super) strategy: String,
    pub(super) resolved_question_id: Option<String>,
}

impl LedgerRow {
    pub(super) fn key(&self) -> (String, String) {
        (self.reference_table.clone(), self.reference_id.clone())
    }
}

pub(super) struct PlannedRepair {
    pub(super) reference: ReferenceRow,
    pub(super) repair: MemoryReferenceRepair,
}

pub(super) struct PlannedInvalidation {
    pub(super) reference: ReferenceRow,
    pub(super) finding: MemoryReferenceFinding,
}

pub(super) struct AuditSnapshot {
    pub(super) counts: MemoryReferenceAuditCounts,
    pub(super) planned_repairs: Vec<PlannedRepair>,
    pub(super) planned_invalidations: Vec<PlannedInvalidation>,
    pub(super) manual_review: Vec<MemoryReferenceFinding>,
    pub(super) stale_ledger: Vec<LedgerRow>,
}

fn question_table_for(record_kind: &str) -> &'static str {
    if record_kind == "web" {
        "web_record_questions"
    } else {
        "conversation_questions"
    }
}

async fn count_references(pool: &SqlitePool, tenant_id: &str, table: &str) -> AppResult<i64> {
    let sql =
        format!("SELECT COUNT(*) FROM {table} WHERE tenant_id = ?1 AND question_id IS NOT NULL");
    Ok(sqlx::query_scalar(AssertSqlSafe(sql))
        .bind(tenant_id)
        .fetch_one(pool)
        .await?)
}

/// 孤儿 = 问题标识非空但无法在同会话内解析到现存分组。
async fn load_orphans(
    pool: &SqlitePool,
    tenant_id: &str,
    table: &'static str,
) -> AppResult<Vec<ReferenceRow>> {
    let mut rows = Vec::new();
    for record_kind in ["session", "web"] {
        let questions = question_table_for(record_kind);
        let sql = format!(
            "SELECT r.id, r.session_id, r.question_id, r.turn_id \
             FROM {table} r \
             LEFT JOIN {questions} q ON q.tenant_id = r.tenant_id AND q.id = r.question_id \
                AND q.session_id = r.session_id \
             WHERE r.tenant_id = ?1 AND r.question_id IS NOT NULL \
                AND r.record_kind = ?2 AND q.id IS NULL"
        );
        let found =
            sqlx::query_as::<_, (String, String, String, Option<String>)>(AssertSqlSafe(sql))
                .bind(tenant_id)
                .bind(record_kind)
                .fetch_all(pool)
                .await?;
        rows.extend(
            found
                .into_iter()
                .map(|(id, session_id, question_id, turn_id)| ReferenceRow {
                    table,
                    id,
                    record_kind: record_kind.to_string(),
                    session_id,
                    question_id,
                    turn_id,
                }),
        );
    }
    Ok(rows)
}

async fn load_ledger(pool: &SqlitePool, tenant_id: &str) -> AppResult<Vec<LedgerRow>> {
    let rows = sqlx::query_as::<_, (String, String, String, String, String, Option<String>)>(
        "SELECT reference_table, reference_id, previous_question_id, resolution, strategy, \
            resolved_question_id \
         FROM memory_reference_audit_findings WHERE tenant_id = ?1",
    )
    .bind(tenant_id)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(
            |(
                reference_table,
                reference_id,
                previous_question_id,
                resolution,
                strategy,
                resolved_question_id,
            )| LedgerRow {
                reference_table,
                reference_id,
                previous_question_id,
                resolution,
                strategy,
                resolved_question_id,
            },
        )
        .collect())
}

/// `None` 表示引用行已不存在；`Some(None)` 表示引用行的问题标识已被清空。
async fn load_reference_question(
    pool: &SqlitePool,
    tenant_id: &str,
    table: &str,
    reference_id: &str,
) -> AppResult<Option<Option<String>>> {
    let sql = format!("SELECT question_id FROM {table} WHERE tenant_id = ?1 AND id = ?2");
    Ok(sqlx::query_scalar::<_, Option<String>>(AssertSqlSafe(sql))
        .bind(tenant_id)
        .bind(reference_id)
        .fetch_optional(pool)
        .await?)
}

/// 用账本把当前引用划分成「已修复」「需人工判定」，并收集已失效的账本条目。
fn classify_ledger(
    ledger: &[LedgerRow],
    orphans: &[ReferenceRow],
    current: &HashMap<(String, String), Option<String>>,
) -> (
    HashSet<(String, String)>,
    Vec<MemoryReferenceFinding>,
    Vec<LedgerRow>,
) {
    let orphan_keys = orphans
        .iter()
        .map(ReferenceRow::key)
        .collect::<HashSet<_>>();
    let orphan_index = orphans
        .iter()
        .map(|row| (row.key(), row))
        .collect::<HashMap<_, _>>();
    let mut repaired_keys = HashSet::new();
    let mut manual_review = Vec::new();
    let mut stale_ledger = Vec::new();
    for entry in ledger {
        let Some(Some(current_question_id)) = current.get(&entry.key()) else {
            stale_ledger.push(entry.clone());
            continue;
        };
        match entry.resolution.as_str() {
            REMAPPED
                if entry.resolved_question_id.as_deref() == Some(current_question_id.as_str())
                    && !orphan_keys.contains(&entry.key()) =>
            {
                repaired_keys.insert(entry.key());
            }
            REMAPPED => stale_ledger.push(entry.clone()),
            INVALIDATED
                if current_question_id == &entry.previous_question_id
                    && orphan_index.contains_key(&entry.key()) =>
            {
                let finding = orphan_index[&entry.key()].finding(&entry.strategy);
                manual_review.push(finding);
            }
            INVALIDATED => stale_ledger.push(entry.clone()),
            _ => stale_ledger.push(entry.clone()),
        }
    }
    (repaired_keys, manual_review, stale_ledger)
}

pub(super) async fn compute_audit(pool: &SqlitePool, tenant_id: &str) -> AppResult<AuditSnapshot> {
    let mut total_references = 0;
    let mut orphans = Vec::new();
    for table in REFERENCE_TABLES {
        total_references += count_references(pool, tenant_id, table).await?;
        orphans.extend(load_orphans(pool, tenant_id, table).await?);
    }
    let ledger = load_ledger(pool, tenant_id).await?;
    let mut current = HashMap::new();
    for entry in &ledger {
        let question_id =
            load_reference_question(pool, tenant_id, &entry.reference_table, &entry.reference_id)
                .await?;
        current.insert(entry.key(), question_id.flatten());
    }
    let (repaired_keys, manual_review, stale_ledger) = classify_ledger(&ledger, &orphans, &current);
    let manual_keys = manual_review
        .iter()
        .map(|finding| {
            (
                finding.reference_table.clone(),
                finding.reference_id.clone(),
            )
        })
        .collect::<HashSet<_>>();
    let mut planned_repairs = Vec::new();
    let mut planned_invalidations = Vec::new();
    for orphan in &orphans {
        if manual_keys.contains(&orphan.key()) {
            continue;
        }
        match resolve_orphan(pool, tenant_id, orphan).await? {
            Resolution::Remapped {
                question_id,
                strategy,
            } => planned_repairs.push(PlannedRepair {
                reference: orphan.clone(),
                repair: MemoryReferenceRepair {
                    reference_table: orphan.table.to_string(),
                    reference_id: orphan.id.clone(),
                    previous_question_id: orphan.question_id.clone(),
                    resolved_question_id: question_id,
                    strategy: strategy.to_string(),
                },
            }),
            Resolution::Invalidated { strategy } => {
                planned_invalidations.push(PlannedInvalidation {
                    reference: orphan.clone(),
                    finding: orphan.finding(strategy),
                });
            }
        }
    }
    let orphan_total = orphans.len() as i64;
    let manual = manual_review.len() as i64;
    let repaired = repaired_keys.len() as i64;
    Ok(AuditSnapshot {
        counts: MemoryReferenceAuditCounts {
            total_references,
            valid: total_references - orphan_total - repaired,
            orphaned: orphan_total - manual,
            repaired,
            manual_review: manual,
        },
        planned_repairs,
        planned_invalidations,
        manual_review,
        stale_ledger,
    })
}
