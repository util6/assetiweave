use std::collections::HashSet;

use chrono::Utc;
use serde_json::{json, Value};
use sqlx::{AssertSqlSafe, SqlitePool};

use crate::backend::application::prelude::*;

pub(crate) const AUDIT_SCHEMA_VERSION: u32 = 1;
pub(crate) const AUDIT_STAGE_COUNT: usize = 10;

#[derive(Debug, Clone)]
pub(crate) struct AuditIssue {
    pub(crate) category: &'static str,
    pub(crate) severity: &'static str,
    pub(crate) auto_repairable: bool,
    pub(crate) affected_count: i64,
    pub(crate) details: Value,
}

pub(crate) async fn audit_conversation_data_sqlx<F>(
    pool: &SqlitePool,
    tenant_id: &str,
    record_kind: Option<&str>,
    source_id: Option<&str>,
    include_resolved: bool,
    cancellation: Option<&tokio_util::sync::CancellationToken>,
    on_progress: &mut F,
) -> AppResult<Value>
where
    F: FnMut(usize, usize, Option<String>),
{
    let families = match record_kind {
        Some("session") => vec![("session", "conversation")],
        Some("web") => vec![("web", "web_record")],
        _ => vec![("session", "conversation"), ("web", "web_record")],
    };
    let family_check_count = families.len() * 7;
    let mut issues = Vec::new();
    let mut progress_index = 0;
    for (family_kind, table_prefix) in families {
        ensure_maintenance_not_cancelled(cancellation)?;
        let sessions = format!("{table_prefix}_sessions");
        let questions = format!("{table_prefix}_questions");
        let question_turns = format!("{table_prefix}_question_turns");
        let turns = format!("{table_prefix}_turns");
        let parts = format!("{table_prefix}_parts");
        let checks = [
            (
                "duplicate_memberships",
                format!(
                    "SELECT COUNT(*) FROM (SELECT qt.turn_id FROM {question_turns} qt JOIN {questions} q ON q.tenant_id = qt.tenant_id AND q.id = qt.question_id JOIN {sessions} s ON s.tenant_id = q.tenant_id AND s.id = q.session_id WHERE qt.tenant_id = ?1 AND (?2 IS NULL OR s.source_id = ?2) GROUP BY qt.turn_id HAVING COUNT(*) > 1)"
                ),
                "error",
                false,
                "a Turn is assigned to multiple Questions",
            ),
            (
                "cross_session_memberships",
                format!(
                    "SELECT COUNT(*) FROM {question_turns} qt JOIN {questions} q ON q.tenant_id = qt.tenant_id AND q.id = qt.question_id JOIN {turns} t ON t.tenant_id = qt.tenant_id AND t.id = qt.turn_id JOIN {sessions} s ON s.tenant_id = q.tenant_id AND s.id = q.session_id WHERE qt.tenant_id = ?1 AND q.session_id != t.session_id AND (?2 IS NULL OR s.source_id = ?2)"
                ),
                "error",
                false,
                "Question and Turn belong to different Sessions",
            ),
            (
                "orphan_memberships",
                format!(
                    "SELECT COUNT(*) FROM {question_turns} qt LEFT JOIN {questions} q ON q.tenant_id = qt.tenant_id AND q.id = qt.question_id LEFT JOIN {turns} t ON t.tenant_id = qt.tenant_id AND t.id = qt.turn_id LEFT JOIN {sessions} s ON s.tenant_id = q.tenant_id AND s.id = q.session_id WHERE qt.tenant_id = ?1 AND (q.id IS NULL OR t.id IS NULL) AND (?2 IS NULL OR (q.id IS NOT NULL AND s.source_id = ?2))"
                ),
                "error",
                true,
                "QuestionTurn references a missing Question or Turn",
            ),
            (
                "orphan_questions",
                format!(
                    "SELECT COUNT(*) FROM {questions} q LEFT JOIN {sessions} s ON s.tenant_id = q.tenant_id AND s.id = q.session_id WHERE q.tenant_id = ?1 AND s.id IS NULL AND ?2 IS NULL"
                ),
                "error",
                true,
                "Question references a missing Session",
            ),
            (
                "orphan_turns",
                format!(
                    "SELECT COUNT(*) FROM {turns} t LEFT JOIN {sessions} s ON s.tenant_id = t.tenant_id AND s.id = t.session_id WHERE t.tenant_id = ?1 AND s.id IS NULL AND ?2 IS NULL"
                ),
                "error",
                true,
                "Turn references a missing Session",
            ),
            (
                "orphan_parts",
                format!(
                    "SELECT COUNT(*) FROM {parts} p LEFT JOIN {turns} t ON t.tenant_id = p.tenant_id AND t.id = p.turn_id WHERE p.tenant_id = ?1 AND t.id IS NULL AND ?2 IS NULL"
                ),
                "error",
                true,
                "Part references a missing Turn",
            ),
            (
                "legacy_split_shell_parts",
                format!(
                    "SELECT COUNT(*) FROM (SELECT p.turn_id, p.source_execution_id FROM {parts} p JOIN {turns} t ON t.tenant_id = p.tenant_id AND t.id = p.turn_id JOIN {sessions} s ON s.tenant_id = t.tenant_id AND s.id = t.session_id WHERE p.tenant_id = ?1 AND p.source_execution_id IS NOT NULL AND (?2 IS NULL OR s.source_id = ?2) GROUP BY p.turn_id, p.source_execution_id HAVING COUNT(*) > 1)"
                ),
                "warning",
                false,
                "one historical execution identifier is stored by multiple Parts",
            ),
        ];
        for (category, query, severity, auto_repairable, detail) in checks {
            ensure_maintenance_not_cancelled(cancellation)?;
            on_progress(
                progress_index,
                family_check_count + 2,
                Some(format!("{family_kind}.{category}")),
            );
            progress_index += 1;
            let count = sqlx::query_scalar::<_, i64>(AssertSqlSafe(query))
                .bind(tenant_id)
                .bind(source_id)
                .fetch_one(pool)
                .await
                .map_err(AppError::external)?;
            if count == 0 {
                continue;
            }
            let issue = AuditIssue {
                category,
                severity,
                auto_repairable,
                affected_count: count,
                details: json!({ "message": detail, "record_kind": family_kind }),
            };
            persist_audit_issue_sqlx(pool, tenant_id, &issue, family_kind, source_id).await?;
            issues.push(issue);
        }
    }

    on_progress(
        family_check_count,
        family_check_count + 2,
        Some("question_snapshot_dependencies".to_string()),
    );
    ensure_maintenance_not_cancelled(cancellation)?;
    let snapshot_count = if record_kind.is_none() && source_id.is_none() {
        let snapshot_query = if include_resolved {
            "SELECT COALESCE(SUM(affected_count), 0) FROM conversation_data_audit_issues WHERE tenant_id = ?1 AND category = 'question_snapshot_dependencies'"
        } else {
            "SELECT COALESCE(SUM(affected_count), 0) FROM conversation_data_audit_issues WHERE tenant_id = ?1 AND category = 'question_snapshot_dependencies' AND status = 'open'"
        };
        sqlx::query_scalar::<_, i64>(snapshot_query)
            .bind(tenant_id)
            .fetch_one(pool)
            .await
            .map_err(AppError::external)?
    } else {
        0
    };
    if snapshot_count > 0 {
        let issue = AuditIssue {
            category: "question_snapshot_dependencies",
            severity: "warning",
            auto_repairable: false,
            affected_count: snapshot_count,
            details: json!({ "message": "Question snapshot history requires review after the contract migration" }),
        };
        issues.push(issue);
    }

    on_progress(
        family_check_count + 1,
        family_check_count + 2,
        Some("search_index_mismatch".to_string()),
    );
    ensure_maintenance_not_cancelled(cancellation)?;
    let search_count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM tenants t LEFT JOIN conversation_search_index_state s ON s.tenant_id = t.id WHERE t.id = ?1 AND (s.tenant_id IS NULL OR s.health != 'ready' OR s.indexed_revision IS NULL OR s.indexed_revision != s.source_revision)",
    )
    .bind(tenant_id)
    .fetch_one(pool)
    .await
    .map_err(AppError::external)?;
    if search_count > 0 {
        let issue = AuditIssue {
            category: "search_index_mismatch",
            severity: "warning",
            auto_repairable: true,
            affected_count: search_count,
            details: json!({ "message": "search index is missing, stale, or incompatible with source revision" }),
        };
        persist_audit_issue_sqlx(pool, tenant_id, &issue, "all", None).await?;
        issues.push(issue);
    }
    on_progress(
        family_check_count + 2,
        family_check_count + 2,
        Some("completed".to_string()),
    );

    let issue_count = issues.len();
    let affected_count = issues.iter().map(|issue| issue.affected_count).sum::<i64>();
    Ok(json!({
        "schema_version": AUDIT_SCHEMA_VERSION,
        "tenant_id": tenant_id,
        "record_kind": record_kind,
        "source_id": source_id,
        "include_resolved": include_resolved,
        "issue_count": issue_count,
        "affected_count": affected_count,
        "issues": issues.iter().map(|issue| json!({
            "category": issue.category,
            "severity": issue.severity,
            "auto_repairable": issue.auto_repairable,
            "affected_count": issue.affected_count,
            "details": issue.details,
        })).collect::<Vec<_>>(),
    }))
}

pub(crate) fn ensure_maintenance_not_cancelled(
    cancellation: Option<&tokio_util::sync::CancellationToken>,
) -> AppResult<()> {
    if cancellation.is_some_and(tokio_util::sync::CancellationToken::is_cancelled) {
        return Err(AppError::Cancelled(
            "conversation data maintenance cancelled".to_string(),
        ));
    }
    Ok(())
}

async fn persist_audit_issue_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    issue: &AuditIssue,
    record_kind: &str,
    source_id: Option<&str>,
) -> AppResult<()> {
    let now = Utc::now().to_rfc3339();
    let fingerprint = conversation_audit_fingerprint(record_kind, issue.category, source_id);
    sqlx::query(
        r#"
        INSERT INTO conversation_data_audit_issues (
            tenant_id, id, category, fingerprint, severity, auto_repairable, status,
            affected_count, sample_ids_json, details_json, first_seen_at, last_seen_at
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'open', ?7, '[]', ?8, ?9, ?9)
        ON CONFLICT DO UPDATE SET
            status = 'open',
            category = excluded.category,
            severity = excluded.severity,
            auto_repairable = excluded.auto_repairable,
            affected_count = excluded.affected_count,
            details_json = excluded.details_json,
            last_seen_at = excluded.last_seen_at,
            resolved_at = NULL
        "#,
    )
    .bind(tenant_id)
    .bind(&fingerprint)
    .bind(issue.category)
    .bind(&fingerprint)
    .bind(issue.severity)
    .bind(issue.auto_repairable)
    .bind(issue.affected_count)
    .bind(issue.details.to_string())
    .bind(now)
    .execute(pool)
    .await
    .map_err(AppError::external)?;
    Ok(())
}

pub(crate) async fn resolve_safe_conversation_audit_issues_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    record_kind: Option<&str>,
    source_id: Option<&str>,
    active_fingerprints: &HashSet<String>,
    cancellation: Option<&tokio_util::sync::CancellationToken>,
) -> AppResult<u64> {
    const SAFE_CATEGORIES: [&str; 5] = [
        "orphan_parts",
        "orphan_memberships",
        "orphan_questions",
        "orphan_turns",
        "search_index_mismatch",
    ];
    let prefixes = match record_kind {
        Some("session") => vec!["session", "all"],
        Some("web") => vec!["web", "all"],
        _ => vec!["session", "web", "all"],
    };
    let now = Utc::now().to_rfc3339();
    let mut resolved = 0;
    for category in SAFE_CATEGORIES {
        for prefix in &prefixes {
            ensure_maintenance_not_cancelled(cancellation)?;
            let fingerprint = conversation_audit_fingerprint(
                prefix,
                category,
                audit_issue_source_scope(category, source_id),
            );
            if active_fingerprints.contains(&fingerprint) {
                continue;
            }
            resolved += sqlx::query(
                r#"
                UPDATE conversation_data_audit_issues
                SET status = 'resolved', resolved_at = ?3, last_seen_at = ?3
                WHERE tenant_id = ?1 AND fingerprint = ?2 AND status = 'open'
                "#,
            )
            .bind(tenant_id)
            .bind(fingerprint)
            .bind(&now)
            .execute(pool)
            .await
            .map_err(AppError::external)?
            .rows_affected();
        }
    }
    ensure_maintenance_not_cancelled(cancellation)?;
    Ok(resolved)
}

pub(crate) fn conversation_audit_fingerprint(
    record_kind: &str,
    category: &str,
    source_id: Option<&str>,
) -> String {
    source_id.map_or_else(
        || format!("{record_kind}:{category}"),
        |source_id| format!("{record_kind}:{category}:source:{source_id}"),
    )
}

pub(crate) fn audit_issue_source_scope<'a>(
    category: &str,
    source_id: Option<&'a str>,
) -> Option<&'a str> {
    if category == "search_index_mismatch" {
        None
    } else {
        source_id
    }
}

pub(crate) async fn apply_safe_conversation_repairs_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    record_kind: Option<&str>,
    source_id: Option<&str>,
    cancellation: Option<&tokio_util::sync::CancellationToken>,
) -> AppResult<Value> {
    let families = match record_kind {
        Some("session") => vec![("conversation", "conversation")],
        Some("web") => vec![("web_record", "web_record")],
        _ => vec![
            ("conversation", "conversation"),
            ("web_record", "web_record"),
        ],
    };
    let mut deleted_parts = 0u64;
    let mut deleted_memberships = 0u64;
    let mut deleted_questions = 0u64;
    let mut deleted_turns = 0u64;
    ensure_maintenance_not_cancelled(cancellation)?;
    let mut tx = pool.begin().await.map_err(AppError::external)?;
    for (prefix, _) in families {
        ensure_maintenance_not_cancelled(cancellation)?;
        let sessions = format!("{prefix}_sessions");
        let questions = format!("{prefix}_questions");
        let question_turns = format!("{prefix}_question_turns");
        let turns = format!("{prefix}_turns");
        let parts = format!("{prefix}_parts");
        ensure_maintenance_not_cancelled(cancellation)?;
        deleted_parts += sqlx::query(AssertSqlSafe(format!(
            "DELETE FROM {parts} WHERE tenant_id = ?1 AND NOT EXISTS (SELECT 1 FROM {turns} t WHERE t.tenant_id = {parts}.tenant_id AND t.id = {parts}.turn_id) AND ?2 IS NULL"
        )))
        .bind(tenant_id)
        .bind(source_id)
        .execute(&mut *tx)
        .await
        .map_err(AppError::external)?
        .rows_affected();
        ensure_maintenance_not_cancelled(cancellation)?;
        deleted_memberships += sqlx::query(AssertSqlSafe(format!(
            "DELETE FROM {question_turns} WHERE tenant_id = ?1 AND (NOT EXISTS (SELECT 1 FROM {questions} q WHERE q.tenant_id = {question_turns}.tenant_id AND q.id = {question_turns}.question_id) OR NOT EXISTS (SELECT 1 FROM {turns} t WHERE t.tenant_id = {question_turns}.tenant_id AND t.id = {question_turns}.turn_id)) AND (?2 IS NULL OR EXISTS (SELECT 1 FROM {questions} q JOIN {sessions} s ON s.tenant_id = q.tenant_id AND s.id = q.session_id WHERE q.tenant_id = {question_turns}.tenant_id AND q.id = {question_turns}.question_id AND s.source_id = ?2))"
        )))
        .bind(tenant_id)
        .bind(source_id)
        .execute(&mut *tx)
        .await
        .map_err(AppError::external)?
        .rows_affected();
        ensure_maintenance_not_cancelled(cancellation)?;
        deleted_questions += sqlx::query(AssertSqlSafe(format!(
            "DELETE FROM {questions} WHERE tenant_id = ?1 AND NOT EXISTS (SELECT 1 FROM {sessions} s WHERE s.tenant_id = {questions}.tenant_id AND s.id = {questions}.session_id) AND ?2 IS NULL"
        )))
        .bind(tenant_id)
        .bind(source_id)
        .execute(&mut *tx)
        .await
        .map_err(AppError::external)?
        .rows_affected();
        ensure_maintenance_not_cancelled(cancellation)?;
        deleted_turns += sqlx::query(AssertSqlSafe(format!(
            "DELETE FROM {turns} WHERE tenant_id = ?1 AND NOT EXISTS (SELECT 1 FROM {sessions} s WHERE s.tenant_id = {turns}.tenant_id AND s.id = {turns}.session_id) AND ?2 IS NULL"
        )))
        .bind(tenant_id)
        .bind(source_id)
        .execute(&mut *tx)
        .await
        .map_err(AppError::external)?
        .rows_affected();
    }
    ensure_maintenance_not_cancelled(cancellation)?;
    crate::backend::store::bump_conversation_search_source_revision_sqlx_tx(&mut tx, tenant_id)
        .await?;
    ensure_maintenance_not_cancelled(cancellation)?;
    tx.commit().await.map_err(AppError::external)?;
    Ok(json!({
        "deleted_parts": deleted_parts,
        "deleted_memberships": deleted_memberships,
        "deleted_questions": deleted_questions,
        "deleted_turns": deleted_turns,
        "scope": { "record_kind": record_kind, "source_id": source_id },
        "non_destructive_categories": [
            "duplicate_memberships",
            "cross_session_memberships",
            "legacy_split_shell_parts",
            "question_snapshot_dependencies"
        ],
    }))
}

pub(crate) fn validate_conversation_maintenance_scope(
    record_kind: Option<&str>,
    source_id: Option<&str>,
) -> AppResult<()> {
    if let Some(record_kind) = record_kind {
        if !matches!(record_kind, "session" | "web") {
            return Err(AppError::Validation(format!(
                "unsupported conversation record kind: {record_kind}"
            )));
        }
    }
    if source_id.is_some_and(|value| value.trim().is_empty()) {
        return Err(AppError::Validation(
            "conversation maintenance source_id must not be empty".to_string(),
        ));
    }
    Ok(())
}
