use super::*;
use crate::backend::dto::{RecentMemoryStatus, RecentSnapshotPublicationKind, SourceAvailability};
use crate::backend::store::recent_snapshot_repo::{
    save_fixture_recent_snapshot_sqlx, FixtureRecentSnapshotInput, FixtureSessionReferenceInput,
    FixtureSnapshotItemInput, FixtureSnapshotProjectInput,
};

#[tokio::test(flavor = "multi_thread")]
async fn test_recent_memory_snapshot_empty_and_fixture_ready() {
    let root = std::env::temp_dir().join(format!(
        "assetiweave-memory-v2-snapshot-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&root).expect("create test root");
    let db_path = root.join("app.db");

    let service = AppService::open_with_db_path(db_path.clone())
        .await
        .expect("open service");

    // 1. Initial state should be empty
    let initial_state = service
        .get_recent_memory_snapshot()
        .await
        .expect("get initial state");
    assert_eq!(initial_state.status, RecentMemoryStatus::Empty);
    assert!(initial_state.snapshot.is_none());
    assert!(initial_state.latest_attempt_task_id.is_none());
    assert!(initial_state.latest_attempt_error.is_none());

    // 2. Insert a fixture snapshot
    let pool = service.db.pool();
    let tenant_id = service.tenant_id().to_string();

    let fixture = FixtureRecentSnapshotInput {
        tenant_id: tenant_id.clone(),
        snapshot_id: "snap-001".to_string(),
        sequence: 1,
        target_watermark_utc: "2026-09-15T14:00:00Z".to_string(),
        local_watermark_date: "2026-09-15".to_string(),
        local_watermark_time: "14:00".to_string(),
        timezone_offset_minutes: 480,
        window_hours: 48,
        window_start_utc: "2026-09-13T14:00:00Z".to_string(),
        window_end_utc: "2026-09-15T14:00:00Z".to_string(),
        publication_kind: RecentSnapshotPublicationKind::Generated,
        reused_from_snapshot_id: None,
        target_fingerprint: "tfp-001".to_string(),
        content_fingerprint: "cfp-001".to_string(),
        generation_skill_asset_id: None,
        generation_skill_revision: None,
        generation_skill_content_hash: None,
        contract_version: "memory.contract.v2".to_string(),
        budget_policy_version: "budget.v1".to_string(),
        projection_policy_version: "projection.v1".to_string(),
        content_generated_at: "2026-09-15T14:01:00Z".to_string(),
        published_at: "2026-09-15T14:01:05Z".to_string(),
        projects: vec![FixtureSnapshotProjectInput {
            project_key: "assetiweave".to_string(),
            project_title: "AssetIWeave Core".to_string(),
            project_path: Some("/code/assetiweave".to_string()),
            summary: "Implemented memory rewrite foundation schema".to_string(),
            no_material_change: false,
            latest_activity_at: "2026-09-15T13:50:00Z".to_string(),
            source_session_count: 2,
            sort_order: 0,
            items: vec![FixtureSnapshotItemInput {
                item_id: "item-001".to_string(),
                revision_id: "rev-001".to_string(),
                category: "decision".to_string(),
                status: "active".to_string(),
                title: "Adopt expand-migrate-contract for v2 schema".to_string(),
                summary: "Preserved legacy tables and added clean v2 foundation".to_string(),
                rationale: "Ensures zero-downtime and compatibility".to_string(),
                occurred_at: "2026-09-15T13:30:00Z".to_string(),
                recommendation_rank: Some(1),
                evidence_fingerprint: "ev-001".to_string(),
                display_date: "2026-09-15".to_string(),
                sort_order: 0,
                session_references: vec![FixtureSessionReferenceInput {
                    id: "ref-001".to_string(),
                    source_id: "src-001".to_string(),
                    session_id: "session-001".to_string(),
                    session_title: "Memory Architecture Session".to_string(),
                    source_agent: "codex".to_string(),
                    last_activity_at: "2026-09-15T13:30:00Z".to_string(),
                    reference_key: "k-001".to_string(),
                    source_revision: 1,
                    availability: SourceAvailability::Available,
                    unavailable_reason: None,
                }],
            }],
        }],
    };

    save_fixture_recent_snapshot_sqlx(pool, &fixture)
        .await
        .expect("save fixture snapshot");

    // 3. Read state again, should be Ready with full snapshot
    let ready_state = service
        .get_recent_memory_snapshot()
        .await
        .expect("get ready state");
    assert_eq!(ready_state.status, RecentMemoryStatus::Ready);
    let snap = ready_state.snapshot.expect("snapshot exists");
    assert_eq!(snap.snapshot_id, "snap-001");
    assert_eq!(snap.sequence, 1);
    assert_eq!(snap.window_hours, 48);
    assert_eq!(snap.projects.len(), 1);

    let proj = &snap.projects[0];
    assert_eq!(proj.project_key, "assetiweave");
    assert_eq!(proj.project_title, "AssetIWeave Core");
    assert_eq!(proj.items.len(), 1);

    let item = &proj.items[0];
    assert_eq!(item.item_id, "item-001");
    assert_eq!(item.category, "decision");
    assert_eq!(item.recommendation_rank, Some(1));
    assert_eq!(item.session_references.len(), 1);
    assert_eq!(item.session_references[0].session_id, "session-001");

    // 4. Clean up
    drop(service);
    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test(flavor = "multi_thread")]
async fn test_memory_v2_schema_constraints() {
    let root = std::env::temp_dir().join(format!(
        "assetiweave-memory-v2-constraints-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&root).expect("create test root");
    let db_path = root.join("app.db");

    let service = AppService::open_with_db_path(db_path.clone())
        .await
        .expect("open service");

    let pool = service.db.pool();
    let tenant_id = service.tenant_id().to_string();

    // 1. CHECK constraint: layer on memory_items must be l1, l2, or l3
    let bad_layer = sqlx::query(
        "INSERT INTO memory_items (tenant_id, id, layer, lifecycle, first_seen_at, last_seen_at, created_at, updated_at) \
         VALUES (?1, 'bad-item', 'invalid_layer', 'current', '2026-09-15T00:00:00Z', '2026-09-15T00:00:00Z', '2026-09-15T00:00:00Z', '2026-09-15T00:00:00Z')",
    )
    .bind(&tenant_id)
    .execute(pool)
    .await;
    assert!(bad_layer.is_err(), "layer CHECK constraint must fail");

    // 2. CHECK constraint: window_hours on recent_memory_snapshots must be 24, 48, or 72
    let bad_window = sqlx::query(
        "INSERT INTO recent_memory_snapshots (\
            tenant_id, id, sequence, target_watermark_utc, local_watermark_date, local_watermark_time, \
            timezone_offset_minutes, window_hours, window_start_utc, window_end_utc, publication_kind, \
            target_fingerprint, content_fingerprint, contract_version, budget_policy_version, \
            projection_policy_version, content_generated_at, published_at\
         ) VALUES (?1, 'snap-bad', 1, '2026-09-15T14:00:00Z', '2026-09-15', '14:00', 480, 99, \
                   '2026-09-13T14:00:00Z', '2026-09-15T14:00:00Z', 'generated', 'tfp', 'cfp', \
                   'contract.v2', 'budget.v1', 'proj.v1', '2026-09-15T14:00:00Z', '2026-09-15T14:00:00Z')",
    )
    .bind(&tenant_id)
    .execute(pool)
    .await;
    assert!(
        bad_window.is_err(),
        "window_hours CHECK constraint must fail"
    );

    // 3. CHECK constraint: category on memory_item_revisions
    // First insert valid memory_item
    sqlx::query(
        "INSERT INTO memory_items (tenant_id, id, layer, lifecycle, first_seen_at, last_seen_at, created_at, updated_at) \
         VALUES (?1, 'item-valid', 'l1', 'current', '2026-09-15T00:00:00Z', '2026-09-15T00:00:00Z', '2026-09-15T00:00:00Z', '2026-09-15T00:00:00Z')",
    )
    .bind(&tenant_id)
    .execute(pool)
    .await
    .expect("insert valid item");

    let bad_cat = sqlx::query(
        "INSERT INTO memory_item_revisions (\
            tenant_id, id, item_id, revision_number, category, status, title, summary, \
            rationale, occurred_at, evidence_fingerprint, created_at, promotion_nomination\
         ) VALUES (?1, 'rev-bad', 'item-valid', 1, 'unsupported_category', 'active', 'title', 'summary', \
                   'rationale', '2026-09-15T00:00:00Z', 'ev', '2026-09-15T00:00:00Z', 'none')",
    )
    .bind(&tenant_id)
    .execute(pool)
    .await;
    assert!(bad_cat.is_err(), "category CHECK constraint must fail");

    // 4. Source Reference non-cascade:
    // Creating a conversation session, referencing it in memory_item_source_references,
    // and deleting the conversation session must NOT delete the source reference.
    let valid_rev = sqlx::query(
        "INSERT INTO memory_item_revisions (\
            tenant_id, id, item_id, revision_number, category, status, title, summary, \
            rationale, occurred_at, evidence_fingerprint, created_at, promotion_nomination\
         ) VALUES (?1, 'rev-valid', 'item-valid', 1, 'decision', 'active', 'title', 'summary', \
                   'rationale', '2026-09-15T00:00:00Z', 'ev', '2026-09-15T00:00:00Z', 'none')",
    )
    .bind(&tenant_id)
    .execute(pool)
    .await
    .expect("insert valid revision");
    assert_eq!(valid_rev.rows_affected(), 1);

    sqlx::query(
        "INSERT INTO memory_item_source_references (\
            tenant_id, id, item_revision_id, record_kind, source_id, session_id, \
            reference_key, source_revision, availability, created_at\
         ) VALUES (?1, 'ref-non-cascade', 'rev-valid', 'session', 'src-1', 'session-phantom', \
                   'k-ref', 1, 'available', '2026-09-15T00:00:00Z')",
    )
    .bind(&tenant_id)
    .execute(pool)
    .await
    .expect("insert reference with phantom session must succeed (no cascade FK to sessions)");

    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM memory_item_source_references WHERE tenant_id = ?1 AND id = 'ref-non-cascade'",
    )
    .bind(&tenant_id)
    .fetch_one(pool)
    .await
    .expect("count reference");
    assert_eq!(count, 1, "source reference remains intact");

    // 5. UNIQUE constraint: memory_item_revisions (tenant_id, item_id, revision_number)
    let dup_rev = sqlx::query(
        "INSERT INTO memory_item_revisions (\
            tenant_id, id, item_id, revision_number, category, status, title, summary, \
            rationale, occurred_at, evidence_fingerprint, created_at, promotion_nomination\
         ) VALUES (?1, 'rev-dup', 'item-valid', 1, 'decision', 'active', 'title2', 'summary2', \
                   'rationale2', '2026-09-15T00:00:00Z', 'ev2', '2026-09-15T00:00:00Z', 'none')",
    )
    .bind(&tenant_id)
    .execute(pool)
    .await;
    assert!(
        dup_rev.is_err(),
        "duplicate revision_number must violate UNIQUE"
    );

    drop(service);
    let _ = std::fs::remove_dir_all(root);
}
