//! 修复计划的事务化落库：重新指向写入引用行 + 账本，无法判定写入失效账本。

use crate::backend::application::prelude::*;
use sqlx::{AssertSqlSafe, SqliteConnection};

use super::scan::{AuditSnapshot, LedgerRow, ReferenceRow};
use super::{MemoryReferenceRepair, INVALIDATED, REMAPPED};

async fn delete_ledger_row(
    connection: &mut SqliteConnection,
    tenant_id: &str,
    entry: &LedgerRow,
) -> AppResult<()> {
    sqlx::query(
        "DELETE FROM memory_reference_audit_findings \
         WHERE tenant_id = ?1 AND reference_table = ?2 AND reference_id = ?3",
    )
    .bind(tenant_id)
    .bind(&entry.reference_table)
    .bind(&entry.reference_id)
    .execute(&mut *connection)
    .await?;
    Ok(())
}

async fn write_finding(
    connection: &mut SqliteConnection,
    tenant_id: &str,
    reference: &ReferenceRow,
    resolution: &str,
    strategy: &str,
    resolved_question_id: Option<&str>,
    now: &str,
) -> AppResult<()> {
    sqlx::query(
        "INSERT INTO memory_reference_audit_findings (\
            tenant_id, id, reference_table, reference_id, record_kind, session_id, \
            previous_question_id, turn_id, resolution, strategy, resolved_question_id, \
            detected_at, updated_at\
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?12) \
         ON CONFLICT(tenant_id, reference_table, reference_id) DO UPDATE SET \
            previous_question_id = excluded.previous_question_id, \
            turn_id = excluded.turn_id, \
            resolution = excluded.resolution, \
            strategy = excluded.strategy, \
            resolved_question_id = excluded.resolved_question_id, \
            detected_at = excluded.detected_at, \
            updated_at = excluded.updated_at",
    )
    .bind(tenant_id)
    .bind(Uuid::new_v4().to_string())
    .bind(reference.table)
    .bind(&reference.id)
    .bind(&reference.record_kind)
    .bind(&reference.session_id)
    .bind(&reference.question_id)
    .bind(&reference.turn_id)
    .bind(resolution)
    .bind(strategy)
    .bind(resolved_question_id)
    .bind(now)
    .execute(&mut *connection)
    .await?;
    Ok(())
}

/// 在调用方开启的事务里落库；返回真正写入的重新指向（漂移的行会被跳过）。
pub(super) async fn apply_snapshot(
    connection: &mut SqliteConnection,
    tenant_id: &str,
    snapshot: &AuditSnapshot,
    now: &str,
) -> AppResult<Vec<MemoryReferenceRepair>> {
    for entry in &snapshot.stale_ledger {
        delete_ledger_row(connection, tenant_id, entry).await?;
    }
    let mut applied = Vec::new();
    for planned in &snapshot.planned_repairs {
        let repair = &planned.repair;
        let sql = format!(
            "UPDATE {} SET question_id = ?1 \
             WHERE tenant_id = ?2 AND id = ?3 AND question_id = ?4",
            planned.reference.table
        );
        let changed = sqlx::query(AssertSqlSafe(sql))
            .bind(&repair.resolved_question_id)
            .bind(tenant_id)
            .bind(&repair.reference_id)
            .bind(&repair.previous_question_id)
            .execute(&mut *connection)
            .await?
            .rows_affected();
        if changed == 0 {
            continue;
        }
        write_finding(
            connection,
            tenant_id,
            &planned.reference,
            REMAPPED,
            &repair.strategy,
            Some(&repair.resolved_question_id),
            now,
        )
        .await?;
        applied.push(repair.clone());
    }
    for planned in &snapshot.planned_invalidations {
        write_finding(
            connection,
            tenant_id,
            &planned.reference,
            INVALIDATED,
            &planned.finding.reason,
            None,
            now,
        )
        .await?;
        if planned.reference.table == "memory_item_source_references" {
            sqlx::query(
                "UPDATE memory_item_source_references \
                 SET availability = 'unavailable', unavailable_reason = 'missing', \
                    unavailable_at = ?1 \
                 WHERE tenant_id = ?2 AND id = ?3 AND question_id = ?4",
            )
            .bind(now)
            .bind(tenant_id)
            .bind(&planned.finding.reference_id)
            .bind(&planned.finding.question_id)
            .execute(&mut *connection)
            .await?;
        }
    }
    Ok(applied)
}
