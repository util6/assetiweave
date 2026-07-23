use crate::backend::dto::AppResult;
use crate::backend::models::{
    MemoryEvidenceSnapshot, MemoryItem, MemoryItemDetail, MemoryItemFilter, MemoryItemRevision,
    MemoryRevisionChangeKind, NewMemoryEvidenceSnapshot, NewMemoryItem,
};
use chrono::Utc;
use sqlx::{sqlite::SqliteRow, QueryBuilder, Row as SqlxRow, Sqlite, SqlitePool};
use std::collections::HashSet;
use uuid::Uuid;

use super::codec::{
    decode_enum, decode_json, decode_optional_enum, encode_enum, encode_json, encode_optional_enum,
};

const MEMORY_ITEM_COLUMNS: &str = r#"
    id, kind, status, title, content_markdown, scope_json, scope_fingerprint,
    origin, origin_run_id, origin_dream_note_id, origin_extraction_id,
    confidence, supersedes_item_id, source_revision, verified_revision,
    stale_reason, created_at, updated_at
"#;

const UPSERT_MEMORY_EVIDENCE_SQL: &str = r#"
    INSERT INTO memory_evidence_snapshots (
        tenant_id, id, record_kind, source_id, session_id, question_id,
        turn_id, part_id, block_id, content_hash, excerpt,
        translated_excerpt, event_time, source_revision,
        source_unavailable, created_at, updated_at
    ) VALUES (
        ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13,
        ?14, ?15, ?16, ?16
    )
    ON CONFLICT(tenant_id, record_kind, block_id, content_hash) DO UPDATE SET
        source_id = excluded.source_id,
        session_id = excluded.session_id,
        question_id = excluded.question_id,
        turn_id = excluded.turn_id,
        part_id = excluded.part_id,
        excerpt = excluded.excerpt,
        translated_excerpt = COALESCE(excluded.translated_excerpt, memory_evidence_snapshots.translated_excerpt),
        event_time = excluded.event_time,
        source_revision = MAX(memory_evidence_snapshots.source_revision, excluded.source_revision),
        source_unavailable = excluded.source_unavailable,
        updated_at = excluded.updated_at
    RETURNING id, record_kind, source_id, session_id, question_id, turn_id,
              part_id, block_id, content_hash, excerpt, translated_excerpt,
              event_time, source_revision, source_unavailable, created_at,
              updated_at
"#;

const LOAD_MEMORY_ITEM_SQL: &str = r#"
    SELECT id, kind, status, title, content_markdown, scope_json,
           scope_fingerprint, origin, origin_run_id, origin_dream_note_id,
           origin_extraction_id, confidence, supersedes_item_id,
           source_revision, verified_revision, stale_reason, created_at,
           updated_at
    FROM memory_items
    WHERE tenant_id = ?1 AND id = ?2
"#;

const LOAD_MEMORY_ITEM_EVIDENCE_SQL: &str = r#"
    SELECT e.id, e.record_kind, e.source_id, e.session_id, e.question_id,
           e.turn_id, e.part_id, e.block_id, e.content_hash, e.excerpt,
           e.translated_excerpt, e.event_time, e.source_revision,
           e.source_unavailable, e.created_at, e.updated_at
    FROM memory_item_evidence link
    JOIN memory_evidence_snapshots e
      ON e.tenant_id = link.tenant_id AND e.id = link.evidence_id
    WHERE link.tenant_id = ?1 AND link.item_id = ?2
    ORDER BY link.sort_order ASC, e.id ASC
"#;

const LOAD_MEMORY_ITEM_REVISIONS_SQL: &str = r#"
    SELECT id, item_id, revision_number, change_kind, kind, status, title,
           content_markdown, scope_json, scope_fingerprint, origin,
           confidence, supersedes_item_id, source_revision,
           verified_revision, stale_reason, changed_at
    FROM memory_item_revisions
    WHERE tenant_id = ?1 AND item_id = ?2
    ORDER BY revision_number DESC
"#;

pub(crate) async fn upsert_memory_evidence_snapshot_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    draft: &NewMemoryEvidenceSnapshot,
) -> AppResult<MemoryEvidenceSnapshot> {
    validate_evidence(draft)?;
    let id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    let record_kind = encode_enum(draft.record_kind)?;
    let row = sqlx::query(UPSERT_MEMORY_EVIDENCE_SQL)
        .bind(tenant_id)
        .bind(id)
        .bind(record_kind)
        .bind(&draft.source_id)
        .bind(draft.session_id.trim())
        .bind(&draft.question_id)
        .bind(&draft.turn_id)
        .bind(&draft.part_id)
        .bind(draft.block_id.trim())
        .bind(draft.content_hash.trim())
        .bind(&draft.excerpt)
        .bind(&draft.translated_excerpt)
        .bind(&draft.event_time)
        .bind(draft.source_revision)
        .bind(bool_value(draft.source_unavailable))
        .bind(now)
        .fetch_one(pool)
        .await
        .map_err(|error| error.to_string())?;
    map_memory_evidence(&row)
}

pub(crate) async fn create_memory_item_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    draft: &NewMemoryItem,
    evidence_ids: &[String],
) -> AppResult<MemoryItemDetail> {
    validate_new_item(draft)?;
    let item_id = Uuid::new_v4().to_string();
    let revision_id = Uuid::new_v4().to_string();
    let now = Utc::now().to_rfc3339();
    let scope_json = encode_json(&draft.scope)?;
    let scope_fingerprint = draft.scope.fingerprint()?;
    let kind = encode_enum(draft.kind)?;
    let status = encode_enum(draft.status)?;
    let origin = encode_enum(draft.origin)?;
    let stale_reason = encode_optional_enum(draft.stale_reason)?;
    let mut tx = pool.begin().await.map_err(|error| error.to_string())?;

    sqlx::query(
        r#"
        INSERT INTO memory_items (
            tenant_id, id, kind, status, title, content_markdown, scope_json,
            scope_fingerprint, origin, origin_run_id, origin_dream_note_id,
            origin_extraction_id, confidence, supersedes_item_id,
            source_revision, verified_revision, stale_reason, created_at, updated_at
        ) VALUES (
            ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13,
            ?14, ?15, ?16, ?17, ?18, ?18
        )
        "#,
    )
    .bind(tenant_id)
    .bind(&item_id)
    .bind(&kind)
    .bind(&status)
    .bind(draft.title.trim())
    .bind(draft.content_markdown.trim())
    .bind(&scope_json)
    .bind(&scope_fingerprint)
    .bind(&origin)
    .bind(&draft.origin_run_id)
    .bind(&draft.origin_dream_note_id)
    .bind(&draft.origin_extraction_id)
    .bind(draft.confidence)
    .bind(&draft.supersedes_item_id)
    .bind(draft.source_revision)
    .bind(draft.verified_revision)
    .bind(&stale_reason)
    .bind(&now)
    .execute(&mut *tx)
    .await
    .map_err(|error| error.to_string())?;

    sqlx::query(
        r#"
        INSERT INTO memory_item_revisions (
            tenant_id, id, item_id, revision_number, change_kind, kind,
            status, title, content_markdown, scope_json, scope_fingerprint,
            origin, confidence, supersedes_item_id, source_revision,
            verified_revision, stale_reason, changed_at
        ) VALUES (
            ?1, ?2, ?3, 1, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12,
            ?13, ?14, ?15, ?16, ?17
        )
        "#,
    )
    .bind(tenant_id)
    .bind(revision_id)
    .bind(&item_id)
    .bind(encode_enum(MemoryRevisionChangeKind::Create)?)
    .bind(&kind)
    .bind(&status)
    .bind(draft.title.trim())
    .bind(draft.content_markdown.trim())
    .bind(&scope_json)
    .bind(&scope_fingerprint)
    .bind(&origin)
    .bind(draft.confidence)
    .bind(&draft.supersedes_item_id)
    .bind(draft.source_revision)
    .bind(draft.verified_revision)
    .bind(&stale_reason)
    .bind(&now)
    .execute(&mut *tx)
    .await
    .map_err(|error| error.to_string())?;

    let mut seen = HashSet::new();
    for (sort_order, evidence_id) in evidence_ids
        .iter()
        .filter(|id| seen.insert((*id).clone()))
        .enumerate()
    {
        let result = sqlx::query(
            r#"
            INSERT INTO memory_item_evidence (
                tenant_id, item_id, evidence_id, sort_order
            )
            SELECT ?1, ?2, ?3, ?4
            WHERE EXISTS (
                SELECT 1
                FROM memory_evidence_snapshots
                WHERE tenant_id = ?1 AND id = ?3
            )
            "#,
        )
        .bind(tenant_id)
        .bind(&item_id)
        .bind(evidence_id)
        .bind(sort_order as i64)
        .execute(&mut *tx)
        .await
        .map_err(|error| error.to_string())?;
        if result.rows_affected() != 1 {
            return Err(format!(
                "memory evidence {evidence_id} was not found for tenant {tenant_id}"
            ));
        }
    }

    tx.commit().await.map_err(|error| error.to_string())?;
    load_memory_item_detail_sqlx(pool, tenant_id, &item_id)
        .await?
        .ok_or_else(|| format!("created memory item {item_id} was not found"))
}

pub(crate) async fn load_memory_item_detail_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    item_id: &str,
) -> AppResult<Option<MemoryItemDetail>> {
    let Some(item_row) = sqlx::query(LOAD_MEMORY_ITEM_SQL)
        .bind(tenant_id)
        .bind(item_id)
        .fetch_optional(pool)
        .await
        .map_err(|error| error.to_string())?
    else {
        return Ok(None);
    };
    let evidence_rows = sqlx::query(LOAD_MEMORY_ITEM_EVIDENCE_SQL)
        .bind(tenant_id)
        .bind(item_id)
        .fetch_all(pool)
        .await
        .map_err(|error| error.to_string())?;
    let revision_rows = sqlx::query(LOAD_MEMORY_ITEM_REVISIONS_SQL)
        .bind(tenant_id)
        .bind(item_id)
        .fetch_all(pool)
        .await
        .map_err(|error| error.to_string())?;

    Ok(Some(MemoryItemDetail {
        item: map_memory_item(&item_row)?,
        evidence: evidence_rows
            .iter()
            .map(map_memory_evidence)
            .collect::<AppResult<Vec<_>>>()?,
        revisions: revision_rows
            .iter()
            .map(map_memory_revision)
            .collect::<AppResult<Vec<_>>>()?,
    }))
}

pub(crate) async fn list_memory_items_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    filter: &MemoryItemFilter,
) -> AppResult<Vec<MemoryItem>> {
    let mut query = QueryBuilder::<Sqlite>::new(format!(
        "SELECT {MEMORY_ITEM_COLUMNS} FROM memory_items WHERE tenant_id = "
    ));
    query.push_bind(tenant_id);
    push_enum_filter(&mut query, "kind", &filter.kinds)?;
    push_enum_filter(&mut query, "status", &filter.statuses)?;
    push_enum_filter(&mut query, "origin", &filter.origins)?;
    if let Some(scope_fingerprint) = filter.scope_fingerprint.as_deref() {
        query.push(" AND scope_fingerprint = ");
        query.push_bind(scope_fingerprint);
    }
    if filter.stale_only {
        query.push(" AND stale_reason IS NOT NULL");
    }
    query.push(" ORDER BY updated_at DESC, id ASC LIMIT ");
    query.push_bind(filter.limit.clamp(1, 200) as i64);
    query.push(" OFFSET ");
    query.push_bind(filter.offset as i64);
    let rows = query
        .build()
        .fetch_all(pool)
        .await
        .map_err(|error| error.to_string())?;
    rows.iter().map(map_memory_item).collect()
}

fn push_enum_filter<T: serde::Serialize + Copy>(
    query: &mut QueryBuilder<Sqlite>,
    column: &str,
    values: &[T],
) -> AppResult<()> {
    if values.is_empty() {
        return Ok(());
    }
    query.push(format!(" AND {column} IN ("));
    let mut separated = query.separated(", ");
    for value in values {
        separated.push_bind(encode_enum(*value)?);
    }
    separated.push_unseparated(")");
    Ok(())
}

fn map_memory_item(row: &SqliteRow) -> AppResult<MemoryItem> {
    Ok(MemoryItem {
        id: row.try_get(0).map_err(|error| error.to_string())?,
        kind: decode_enum(row.try_get(1).map_err(|error| error.to_string())?)?,
        status: decode_enum(row.try_get(2).map_err(|error| error.to_string())?)?,
        title: row.try_get(3).map_err(|error| error.to_string())?,
        content_markdown: row.try_get(4).map_err(|error| error.to_string())?,
        scope: decode_json(row.try_get(5).map_err(|error| error.to_string())?)?,
        scope_fingerprint: row.try_get(6).map_err(|error| error.to_string())?,
        origin: decode_enum(row.try_get(7).map_err(|error| error.to_string())?)?,
        origin_run_id: row.try_get(8).map_err(|error| error.to_string())?,
        origin_dream_note_id: row.try_get(9).map_err(|error| error.to_string())?,
        origin_extraction_id: row.try_get(10).map_err(|error| error.to_string())?,
        confidence: row.try_get(11).map_err(|error| error.to_string())?,
        supersedes_item_id: row.try_get(12).map_err(|error| error.to_string())?,
        source_revision: row.try_get(13).map_err(|error| error.to_string())?,
        verified_revision: row.try_get(14).map_err(|error| error.to_string())?,
        stale_reason: decode_optional_enum(row.try_get(15).map_err(|error| error.to_string())?)?,
        created_at: row.try_get(16).map_err(|error| error.to_string())?,
        updated_at: row.try_get(17).map_err(|error| error.to_string())?,
    })
}

fn map_memory_evidence(row: &SqliteRow) -> AppResult<MemoryEvidenceSnapshot> {
    Ok(MemoryEvidenceSnapshot {
        id: row.try_get(0).map_err(|error| error.to_string())?,
        record_kind: decode_enum(row.try_get(1).map_err(|error| error.to_string())?)?,
        source_id: row.try_get(2).map_err(|error| error.to_string())?,
        session_id: row.try_get(3).map_err(|error| error.to_string())?,
        question_id: row.try_get(4).map_err(|error| error.to_string())?,
        turn_id: row.try_get(5).map_err(|error| error.to_string())?,
        part_id: row.try_get(6).map_err(|error| error.to_string())?,
        block_id: row.try_get(7).map_err(|error| error.to_string())?,
        content_hash: row.try_get(8).map_err(|error| error.to_string())?,
        excerpt: row.try_get(9).map_err(|error| error.to_string())?,
        translated_excerpt: row.try_get(10).map_err(|error| error.to_string())?,
        event_time: row.try_get(11).map_err(|error| error.to_string())?,
        source_revision: row.try_get(12).map_err(|error| error.to_string())?,
        source_unavailable: row
            .try_get::<i64, _>(13)
            .map_err(|error| error.to_string())?
            == 1,
        created_at: row.try_get(14).map_err(|error| error.to_string())?,
        updated_at: row.try_get(15).map_err(|error| error.to_string())?,
    })
}

fn map_memory_revision(row: &SqliteRow) -> AppResult<MemoryItemRevision> {
    Ok(MemoryItemRevision {
        id: row.try_get(0).map_err(|error| error.to_string())?,
        item_id: row.try_get(1).map_err(|error| error.to_string())?,
        revision_number: row.try_get(2).map_err(|error| error.to_string())?,
        change_kind: decode_enum(row.try_get(3).map_err(|error| error.to_string())?)?,
        kind: decode_enum(row.try_get(4).map_err(|error| error.to_string())?)?,
        status: decode_enum(row.try_get(5).map_err(|error| error.to_string())?)?,
        title: row.try_get(6).map_err(|error| error.to_string())?,
        content_markdown: row.try_get(7).map_err(|error| error.to_string())?,
        scope: decode_json(row.try_get(8).map_err(|error| error.to_string())?)?,
        scope_fingerprint: row.try_get(9).map_err(|error| error.to_string())?,
        origin: decode_enum(row.try_get(10).map_err(|error| error.to_string())?)?,
        confidence: row.try_get(11).map_err(|error| error.to_string())?,
        supersedes_item_id: row.try_get(12).map_err(|error| error.to_string())?,
        source_revision: row.try_get(13).map_err(|error| error.to_string())?,
        verified_revision: row.try_get(14).map_err(|error| error.to_string())?,
        stale_reason: decode_optional_enum(row.try_get(15).map_err(|error| error.to_string())?)?,
        changed_at: row.try_get(16).map_err(|error| error.to_string())?,
    })
}

fn validate_evidence(draft: &NewMemoryEvidenceSnapshot) -> AppResult<()> {
    if draft.session_id.trim().is_empty() {
        return Err("memory evidence session_id is required".to_string());
    }
    if draft.block_id.trim().is_empty() {
        return Err("memory evidence block_id is required".to_string());
    }
    if draft.content_hash.trim().is_empty() {
        return Err("memory evidence content_hash is required".to_string());
    }
    if draft.excerpt.chars().count() > 8192 {
        return Err("memory evidence excerpt exceeds 8192 characters".to_string());
    }
    if draft.source_revision < 0 {
        return Err("memory evidence source_revision cannot be negative".to_string());
    }
    Ok(())
}

fn validate_new_item(draft: &NewMemoryItem) -> AppResult<()> {
    if draft.title.trim().is_empty() {
        return Err("memory item title is required".to_string());
    }
    if draft.content_markdown.trim().is_empty() {
        return Err("memory item content is required".to_string());
    }
    if draft.source_revision < 0
        || draft.verified_revision < 0
        || draft.verified_revision > draft.source_revision
    {
        return Err("memory item revisions are invalid".to_string());
    }
    if draft
        .confidence
        .is_some_and(|confidence| !(0.0..=1.0).contains(&confidence))
    {
        return Err("memory item confidence must be between 0 and 1".to_string());
    }
    Ok(())
}

fn bool_value(value: bool) -> i64 {
    if value {
        1
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::models::{
        MemoryEvidenceRecordKind, MemoryItemKind, MemoryItemOrigin, MemoryItemStatus, MemoryScope,
    };
    use uuid::Uuid;

    #[test]
    fn memory_repo_deduplicates_evidence_snapshots() {
        let (database, db_path) = test_database("evidence-dedupe");
        let draft = test_evidence();

        let (first, second, count) = database
            .block_on(async {
                let first =
                    upsert_memory_evidence_snapshot_sqlx(database.pool(), "default", &draft)
                        .await?;
                let second =
                    upsert_memory_evidence_snapshot_sqlx(database.pool(), "default", &draft)
                        .await?;
                let count = sqlx::query_scalar::<_, i64>(
                    "SELECT COUNT(*) FROM memory_evidence_snapshots WHERE tenant_id = 'default'",
                )
                .fetch_one(database.pool())
                .await
                .map_err(|error| error.to_string())?;
                AppResult::Ok((first, second, count))
            })
            .expect("deduplicate evidence");

        assert_eq!(first.id, second.id);
        assert_eq!(count, 1);
        cleanup(database, &db_path);
    }

    #[test]
    fn memory_repo_creates_item_revision_and_evidence_atomically() {
        let (database, db_path) = test_database("item-create");

        let detail = database
            .block_on(async {
                let evidence = upsert_memory_evidence_snapshot_sqlx(
                    database.pool(),
                    "default",
                    &test_evidence(),
                )
                .await?;
                create_memory_item_sqlx(database.pool(), "default", &test_item(), &[evidence.id])
                    .await
            })
            .expect("create memory item");

        assert_eq!(detail.item.status, MemoryItemStatus::Active);
        assert_eq!(detail.evidence.len(), 1);
        assert_eq!(detail.revisions.len(), 1);
        assert_eq!(detail.revisions[0].revision_number, 1);
        cleanup(database, &db_path);
    }

    #[test]
    fn memory_repo_rejects_cross_tenant_evidence_and_rolls_back_item() {
        let (database, db_path) = test_database("tenant-boundary");

        let (error, tenant_b_count) = database
            .block_on(async {
                sqlx::query(
                    "INSERT INTO tenants (id, slug, name, kind, status, created_at, updated_at) VALUES ('tenant-b', 'tenant-b', 'Tenant B', 'local_workspace', 'active', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
                )
                .execute(database.pool())
                .await
                .map_err(|error| error.to_string())?;
                let evidence = upsert_memory_evidence_snapshot_sqlx(
                    database.pool(),
                    "default",
                    &test_evidence(),
                )
                .await?;
                let error = create_memory_item_sqlx(
                    database.pool(),
                    "tenant-b",
                    &test_item(),
                    &[evidence.id],
                )
                .await
                .expect_err("cross-tenant evidence must fail");
                let tenant_b_count = sqlx::query_scalar::<_, i64>(
                    "SELECT COUNT(*) FROM memory_items WHERE tenant_id = 'tenant-b'",
                )
                .fetch_one(database.pool())
                .await
                .map_err(|error| error.to_string())?;
                AppResult::Ok((error, tenant_b_count))
            })
            .expect("verify tenant boundary");

        assert!(error.contains("evidence") || error.contains("FOREIGN KEY"));
        assert_eq!(tenant_b_count, 0);
        cleanup(database, &db_path);
    }

    #[test]
    fn memory_repo_filters_status_scope_and_staleness_with_stable_pagination() {
        let (database, db_path) = test_database("filters");
        let mut candidate = test_item();
        candidate.status = MemoryItemStatus::Candidate;
        candidate.kind = MemoryItemKind::Method;
        let mut stale = test_item();
        stale.title = "Stale decision".to_string();
        stale.stale_reason = Some(crate::backend::models::MemoryStaleReason::EvidenceChanged);

        let items = database
            .block_on(async {
                create_memory_item_sqlx(database.pool(), "default", &test_item(), &[]).await?;
                create_memory_item_sqlx(database.pool(), "default", &candidate, &[]).await?;
                create_memory_item_sqlx(database.pool(), "default", &stale, &[]).await?;
                let filtered = list_memory_items_sqlx(
                    database.pool(),
                    "default",
                    &MemoryItemFilter {
                        statuses: vec![MemoryItemStatus::Active],
                        scope_fingerprint: Some(test_item().scope.fingerprint()?),
                        stale_only: true,
                        limit: 10,
                        ..MemoryItemFilter::default()
                    },
                )
                .await?;
                AppResult::Ok(filtered)
            })
            .expect("filter memory items");

        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title, "Stale decision");
        cleanup(database, &db_path);
    }

    fn test_evidence() -> NewMemoryEvidenceSnapshot {
        NewMemoryEvidenceSnapshot {
            record_kind: MemoryEvidenceRecordKind::Session,
            source_id: Some("codex".to_string()),
            session_id: "session-1".to_string(),
            question_id: Some("question-1".to_string()),
            turn_id: Some("turn-1".to_string()),
            part_id: Some("part-1".to_string()),
            block_id: "part-1".to_string(),
            content_hash: "sha256:one".to_string(),
            excerpt: "Use the shared AppService boundary.".to_string(),
            translated_excerpt: None,
            event_time: Some("2026-01-01T00:00:00Z".to_string()),
            source_revision: 3,
            source_unavailable: false,
        }
    }

    fn test_item() -> NewMemoryItem {
        NewMemoryItem {
            kind: MemoryItemKind::Decision,
            status: MemoryItemStatus::Active,
            title: "Keep one application workflow boundary".to_string(),
            content_markdown: "Desktop and CLI both call AppService.".to_string(),
            scope: MemoryScope {
                project_path: Some("~/assetiweave".to_string()),
                ..MemoryScope::default()
            },
            origin: MemoryItemOrigin::Manual,
            origin_run_id: None,
            origin_dream_note_id: None,
            origin_extraction_id: None,
            confidence: Some(1.0),
            supersedes_item_id: None,
            source_revision: 3,
            verified_revision: 3,
            stale_reason: None,
        }
    }

    fn test_database(label: &str) -> (crate::backend::store::Database, std::path::PathBuf) {
        let db_path = std::env::temp_dir().join(format!(
            "assetiweave-memory-repo-{label}-{}.sqlite",
            Uuid::new_v4()
        ));
        let database = crate::backend::store::Database::open(&db_path).expect("open database");
        (database, db_path)
    }

    fn cleanup(database: crate::backend::store::Database, db_path: &std::path::Path) {
        drop(database);
        let _ = std::fs::remove_file(db_path);
        let _ = std::fs::remove_file(db_path.with_extension("sqlite-wal"));
        let _ = std::fs::remove_file(db_path.with_extension("sqlite-shm"));
    }
}
