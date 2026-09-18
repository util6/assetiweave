use super::*;
use crate::backend::application::AppService;
use std::fs;

async fn setup_test_service() -> (AppService, std::path::PathBuf) {
    let root = std::env::temp_dir().join(format!(
        "assetiweave-project-consolidation-test-{}",
        uuid::Uuid::new_v4()
    ));
    fs::create_dir_all(&root).expect("create test root");
    let db_path = root.join("app.db");

    let service = AppService::open_with_db_path(db_path)
        .await
        .expect("open service");

    (service, root)
}

async fn seed_snapshot(
    pool: &SqlitePool,
    snapshot_id: &str,
    sequence: i64,
    watermark_utc: &str,
    pub_kind: &str,
) {
    sqlx::query(
        "INSERT INTO recent_memory_snapshots (\
                tenant_id, id, sequence, target_watermark_utc, local_watermark_date, \
                local_watermark_time, timezone_offset_minutes, window_hours, window_start_utc, \
                window_end_utc, publication_kind, reused_from_snapshot_id, target_fingerprint, \
                content_fingerprint, generation_skill_asset_id, generation_skill_revision, \
                generation_skill_content_hash, contract_version, budget_policy_version, \
                projection_policy_version, content_generated_at, published_at\
             ) VALUES (\
                'default', ?1, ?2, ?3, '2026-09-15', '14:00', 0, 48, '2026-09-13T14:00:00Z', \
                ?3, ?4, NULL, 'tfp', 'cfp', NULL, NULL, NULL, 'v2', 'budget.v1', 'proj.v1', ?3, ?3\
             )",
    )
    .bind(snapshot_id)
    .bind(sequence)
    .bind(watermark_utc)
    .bind(pub_kind)
    .execute(pool)
    .await
    .expect("seed snapshot");
}

async fn seed_l1_item(
    pool: &SqlitePool,
    item_id: &str,
    rev_id: &str,
    project_key: &str,
    category: &str,
    status: &str,
    nomination: &str,
    title: &str,
    evidence_fp: &str,
) {
    let now = Utc::now().to_rfc3339();
    sqlx::query(
        "INSERT INTO memory_items (\
                tenant_id, id, layer, project_key, current_revision_id, lifecycle, \
                first_seen_at, last_seen_at, created_at, updated_at\
             ) VALUES ('default', ?1, 'l1', ?2, ?3, 'current', ?4, ?4, ?4, ?4)",
    )
    .bind(item_id)
    .bind(project_key)
    .bind(rev_id)
    .bind(&now)
    .execute(pool)
    .await
    .expect("seed l1 item");

    sqlx::query(
            "INSERT INTO memory_item_revisions (\
                tenant_id, id, item_id, revision_number, category, status, title, \
                summary, rationale, recommendation_rank, promotion_nomination, \
                occurred_at, evidence_fingerprint, generated_by_snapshot_id, \
                supersedes_revision_id, created_at\
             ) VALUES ('default', ?1, ?2, 1, ?3, ?4, ?5, 'summary', 'rationale', NULL, ?6, ?7, ?8, NULL, NULL, ?7)",
        )
        .bind(rev_id)
        .bind(item_id)
        .bind(category)
        .bind(status)
        .bind(title)
        .bind(nomination)
        .bind(&now)
        .bind(evidence_fp)
        .execute(pool)
        .await
        .expect("seed l1 revision");
}

async fn seed_source_reference(
    pool: &SqlitePool,
    rev_id: &str,
    ref_key: &str,
    is_user_content: bool,
    is_available: bool,
) {
    let ref_id = format!("ref-{}", uuid::Uuid::new_v4());
    let now = Utc::now().to_rfc3339();
    let q_id = if is_user_content {
        Some("q-1".to_string())
    } else {
        None
    };
    let avail = if is_available {
        "available"
    } else {
        "unavailable"
    };

    sqlx::query(
            "INSERT INTO memory_item_source_references (\
                tenant_id, id, item_revision_id, record_kind, source_id, session_id, \
                question_id, turn_id, part_id, node_id, node_order, reference_key, \
                source_revision, availability, unavailable_reason, unavailable_at, created_at\
             ) VALUES ('default', ?1, ?2, 'session', 'src-1', 'sess-1', ?3, NULL, NULL, NULL, NULL, ?4, 1, ?5, NULL, NULL, ?6)",
        )
        .bind(&ref_id)
        .bind(rev_id)
        .bind(q_id)
        .bind(ref_key)
        .bind(avail)
        .bind(&now)
        .execute(pool)
        .await
        .expect("seed source reference");
}

async fn seed_observation(
    pool: &SqlitePool,
    item_id: &str,
    rev_id: &str,
    snapshot_id: &str,
    nomination: &str,
    evidence_fp: &str,
    project_key: &str,
    observed_at: &str,
) {
    let obs_id = format!("obs-{}", uuid::Uuid::new_v4());
    sqlx::query(
        "INSERT INTO memory_promotion_observations (\
                tenant_id, id, item_id, item_revision_id, snapshot_id, nomination, \
                evidence_fingerprint, project_key, observed_at\
             ) VALUES ('default', ?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
    )
    .bind(&obs_id)
    .bind(item_id)
    .bind(rev_id)
    .bind(snapshot_id)
    .bind(nomination)
    .bind(evidence_fp)
    .bind(project_key)
    .bind(observed_at)
    .execute(pool)
    .await
    .expect("seed observation");
}

#[tokio::test]
async fn test_m35_l2_03_decision_promotes_with_one_observation() {
    let (service, _root) = setup_test_service().await;
    let pool = service.db.pool();

    seed_snapshot(pool, "snap-1", 1, "2026-09-15T02:00:00Z", "generated").await;
    seed_l1_item(
        pool,
        "item-dec-1",
        "rev-dec-1",
        "proj-alpha",
        "decision",
        "active",
        "project_decision",
        "Decide SQLite Engine",
        "fp-1",
    )
    .await;
    seed_source_reference(pool, "rev-dec-1", "ref-user-1", true, true).await;
    seed_observation(
        pool,
        "item-dec-1",
        "rev-dec-1",
        "snap-1",
        "project_decision",
        "fp-1",
        "proj-alpha",
        "2026-09-15T02:00:00Z",
    )
    .await;

    let candidates = evaluate_l2_candidates(pool, "default", "proj-alpha")
        .await
        .expect("evaluate candidates");

    assert_eq!(
        candidates.len(),
        1,
        "M35-L2-03: project_decision with 1 generated observation must be a candidate"
    );
    assert_eq!(
        candidates[0].nomination,
        MemoryPromotionNomination::ProjectDecision
    );
    assert_eq!(candidates[0].observation_count, 1);
}

#[tokio::test]
async fn test_m35_l2_03_decision_without_user_content_rejected() {
    let (service, _root) = setup_test_service().await;
    let pool = service.db.pool();

    seed_snapshot(pool, "snap-1", 1, "2026-09-15T02:00:00Z", "generated").await;
    seed_l1_item(
        pool,
        "item-dec-2",
        "rev-dec-2",
        "proj-alpha",
        "decision",
        "active",
        "project_decision",
        "Decide Without User Content",
        "fp-1",
    )
    .await;
    // Reference is available but NOT user content
    seed_source_reference(pool, "rev-dec-2", "ref-agent-1", false, true).await;
    seed_observation(
        pool,
        "item-dec-2",
        "rev-dec-2",
        "snap-1",
        "project_decision",
        "fp-1",
        "proj-alpha",
        "2026-09-15T02:00:00Z",
    )
    .await;

    let candidates = evaluate_l2_candidates(pool, "default", "proj-alpha")
        .await
        .expect("evaluate candidates");

    assert_eq!(
        candidates.len(),
        0,
        "Decision without available user content reference must be rejected"
    );
}

#[tokio::test]
async fn test_m35_l2_04_blocker_requires_two_generated_snapshots_with_new_evidence() {
    let (service, _root) = setup_test_service().await;
    let pool = service.db.pool();

    seed_snapshot(pool, "snap-1", 1, "2026-09-15T02:00:00Z", "generated").await;
    seed_l1_item(
        pool,
        "item-blk-1",
        "rev-blk-1",
        "proj-alpha",
        "blocker",
        "blocked",
        "recurring_blocker",
        "Docker Network Flake",
        "fp-blk-1",
    )
    .await;
    seed_source_reference(pool, "rev-blk-1", "ref-user-1", true, true).await;
    seed_observation(
        pool,
        "item-blk-1",
        "rev-blk-1",
        "snap-1",
        "recurring_blocker",
        "fp-blk-1",
        "proj-alpha",
        "2026-09-15T02:00:00Z",
    )
    .await;

    // 仅 1 次观察：不满足
    let candidates1 = evaluate_l2_candidates(pool, "default", "proj-alpha")
        .await
        .expect("evaluate candidates");
    assert_eq!(
        candidates1.len(),
        0,
        "Single observation of blocker must not qualify"
    );

    // 中间插入 reused snapshot: reused snapshot 不增加观察
    seed_snapshot(pool, "snap-2", 2, "2026-09-15T08:00:00Z", "reused").await;
    let candidates2 = evaluate_l2_candidates(pool, "default", "proj-alpha")
        .await
        .expect("evaluate candidates");
    assert_eq!(
        candidates2.len(),
        0,
        "Reused snapshot does not qualify blocker"
    );

    // 第 2 次 generated snapshot，但指纹没有变化: 依然不满足
    seed_snapshot(pool, "snap-3", 3, "2026-09-15T14:00:00Z", "generated").await;
    seed_observation(
        pool,
        "item-blk-1",
        "rev-blk-1",
        "snap-3",
        "recurring_blocker",
        "fp-blk-1", // 相同指纹！
        "proj-alpha",
        "2026-09-15T14:00:00Z",
    )
    .await;
    let candidates3 = evaluate_l2_candidates(pool, "default", "proj-alpha")
        .await
        .expect("evaluate candidates");
    assert_eq!(
        candidates3.len(),
        0,
        "Identical fingerprint without new evidence must not qualify"
    );

    // 第 3 次 generated snapshot，指纹发生变化且有新证据: 满足晋升！
    seed_snapshot(pool, "snap-4", 4, "2026-09-16T02:00:00Z", "generated").await;
    seed_observation(
        pool,
        "item-blk-1",
        "rev-blk-1",
        "snap-4",
        "recurring_blocker",
        "fp-blk-2", // 新指纹！
        "proj-alpha",
        "2026-09-16T02:00:00Z",
    )
    .await;

    let candidates4 = evaluate_l2_candidates(pool, "default", "proj-alpha")
        .await
        .expect("evaluate candidates");
    assert_eq!(
        candidates4.len(),
        1,
        "Blocker with 2 distinct generated observations and changed fingerprint must qualify"
    );
    assert_eq!(
        candidates4[0].nomination,
        MemoryPromotionNomination::RecurringBlocker
    );
    assert_eq!(candidates4[0].observation_count, 3);
}

#[tokio::test]
async fn test_m35_l2_05_progress_and_completion_do_not_promote() {
    let (service, _root) = setup_test_service().await;
    let pool = service.db.pool();

    seed_snapshot(pool, "snap-1", 1, "2026-09-15T02:00:00Z", "generated").await;
    seed_snapshot(pool, "snap-2", 2, "2026-09-15T14:00:00Z", "generated").await;

    // 1. Progress 项 (nomination = none): 绝不作为合格候选
    seed_l1_item(
        pool,
        "item-prog-1",
        "rev-prog-1",
        "proj-alpha",
        "progress",
        "active",
        "none",
        "Refactored CSS styles",
        "fp-prog-1",
    )
    .await;
    seed_source_reference(pool, "rev-prog-1", "ref-1", true, true).await;

    // 2. Completed 项: 即使 nomination 为 recurring_blocker 且有 2 次历史观察，一旦 completed 也不再晋升
    seed_l1_item(
        pool,
        "item-comp-1",
        "rev-comp-1",
        "proj-alpha",
        "blocker",
        "completed",
        "recurring_blocker",
        "Resolved Flaky Test",
        "fp-comp-2",
    )
    .await;
    seed_source_reference(pool, "rev-comp-1", "ref-2", true, true).await;
    seed_observation(
        pool,
        "item-comp-1",
        "rev-comp-1",
        "snap-1",
        "recurring_blocker",
        "fp-comp-1",
        "proj-alpha",
        "2026-09-15T02:00:00Z",
    )
    .await;
    seed_observation(
        pool,
        "item-comp-1",
        "rev-comp-1",
        "snap-2",
        "recurring_blocker",
        "fp-comp-2",
        "proj-alpha",
        "2026-09-15T14:00:00Z",
    )
    .await;

    let candidates = evaluate_l2_candidates(pool, "default", "proj-alpha")
        .await
        .expect("evaluate candidates");
    assert_eq!(
        candidates.len(),
        0,
        "Progress items and completed blockers must NEVER promote to L2 (M35-L2-05)"
    );
}

#[tokio::test]
async fn test_m35_l2_02_unassigned_does_not_promote() {
    let (service, _root) = setup_test_service().await;
    let pool = service.db.pool();

    seed_snapshot(pool, "snap-1", 1, "2026-09-15T02:00:00Z", "generated").await;
    seed_l1_item(
        pool,
        "item-unassigned",
        "rev-unassigned",
        "unassigned",
        "decision",
        "active",
        "project_decision",
        "Unassigned Decision",
        "fp-u",
    )
    .await;
    seed_source_reference(pool, "rev-unassigned", "ref-u", true, true).await;
    seed_observation(
        pool,
        "item-unassigned",
        "rev-unassigned",
        "snap-1",
        "project_decision",
        "fp-u",
        "unassigned",
        "2026-09-15T02:00:00Z",
    )
    .await;

    let candidates = evaluate_l2_candidates(pool, "default", "unassigned")
        .await
        .expect("evaluate candidates");
    assert_eq!(
        candidates.len(),
        0,
        "unassigned items must NEVER promote to L2 (M35-L2-02)"
    );
}

#[tokio::test]
async fn test_m35_l2_06_zero_candidates_triggers_zero_agent_calls() {
    let (service, _root) = setup_test_service().await;
    let pool = service.db.pool();
    let lock_map = ProjectConsolidationLockMap::new();

    // 当没有候选时，传入的 Agent runner 若被调用则直接 panic
    let result = reconcile_project_consolidation(
        pool,
        "default",
        "proj-empty",
        Some("/path/empty"),
        &lock_map,
        Some(|_input: ProjectConsolidationInput| async move {
            panic!("Agent runner MUST NOT be called when there are 0 candidates! (M35-L2-06)");
            #[allow(unreachable_code)]
            Ok::<_, AppError>(ProjectConsolidationResult { operations: vec![] })
        }),
    )
    .await
    .expect("consolidation with zero candidates");

    assert!(
        result.is_none(),
        "Zero candidates should result in None view"
    );
}

#[tokio::test]
async fn test_memory_v2_frozen_project_input_rejects_changed_sqlite_evidence() {
    let (service, _root) = setup_test_service().await;
    let pool = service.db.pool();
    let lock_map = ProjectConsolidationLockMap::new();

    seed_snapshot(pool, "snap-frozen", 1, "2026-09-15T02:00:00Z", "generated").await;
    seed_l1_item(
        pool,
        "item-frozen",
        "rev-frozen",
        "proj-frozen",
        "decision",
        "active",
        "project_decision",
        "Frozen decision",
        "fp-frozen",
    )
    .await;
    seed_source_reference(pool, "rev-frozen", "ref-frozen", true, true).await;
    seed_observation(
        pool,
        "item-frozen",
        "rev-frozen",
        "snap-frozen",
        "project_decision",
        "fp-frozen",
        "proj-frozen",
        "2026-09-15T02:00:00Z",
    )
    .await;

    let frozen_input =
        load_project_consolidation_input(pool, "default", "proj-frozen", Some("/tmp/frozen"))
            .await
            .expect("freeze project input");
    sqlx::query(
        "UPDATE memory_item_revisions SET summary = 'changed after enqueue' \
             WHERE tenant_id = 'default' AND id = 'rev-frozen'",
    )
    .execute(pool)
    .await
    .expect("change source evidence");

    let error = reconcile_project_consolidation_with_lease(
        pool,
        "default",
        "proj-frozen",
        Some("/tmp/frozen"),
        &lock_map,
        Some(|_| async move {
            panic!("stale frozen input must not call the Agent");
            #[allow(unreachable_code)]
            Ok::<_, AppError>(ProjectConsolidationResult { operations: vec![] })
        }),
        None,
        Some(frozen_input),
    )
    .await
    .expect_err("changed evidence must mark the Work Order stale");

    assert_eq!(error.code(), "MEMORY_WORK_ORDER_STALE");
}

#[tokio::test]
async fn test_m35_l2_06_serial_per_project_and_failure_preserves_current_l2() {
    let (service, _root) = setup_test_service().await;
    let pool = service.db.pool();
    let lock_map = ProjectConsolidationLockMap::new();

    // 1. 预先在数据库中插入一个已存在的 current L2 item
    let existing_l2_item_id = "item-l2-existing";
    let existing_l2_rev_id = "rev-l2-existing";
    let now = Utc::now().to_rfc3339();

    sqlx::query(
        "INSERT INTO memory_items (\
                tenant_id, id, layer, project_key, current_revision_id, lifecycle, \
                first_seen_at, last_seen_at, created_at, updated_at\
             ) VALUES ('default', ?1, 'l2', 'proj-alpha', ?2, 'current', ?3, ?3, ?3, ?3)",
    )
    .bind(existing_l2_item_id)
    .bind(existing_l2_rev_id)
    .bind(&now)
    .execute(pool)
    .await
    .expect("insert existing l2 item");

    sqlx::query(
        "INSERT INTO memory_item_revisions (\
                tenant_id, id, item_id, revision_number, category, status, title, \
                summary, rationale, recommendation_rank, promotion_nomination, \
                occurred_at, evidence_fingerprint, generated_by_snapshot_id, \
                supersedes_revision_id, created_at\
             ) VALUES ('default', ?1, ?2, 1, 'decision', 'active', 'Pre-existing Decision', \
                       'Summary existing', 'Rationale', NULL, 'none', ?3, 'fp-ex', NULL, NULL, ?3)",
    )
    .bind(existing_l2_rev_id)
    .bind(existing_l2_item_id)
    .bind(&now)
    .execute(pool)
    .await
    .expect("insert existing l2 revision");

    // 2. 插入一个新合格候选
    seed_snapshot(pool, "snap-1", 1, "2026-09-15T02:00:00Z", "generated").await;
    seed_l1_item(
        pool,
        "item-dec-new",
        "rev-dec-new",
        "proj-alpha",
        "decision",
        "active",
        "project_decision",
        "New Alpha Decision",
        "fp-new",
    )
    .await;
    seed_source_reference(pool, "rev-dec-new", "ref-user-new", true, true).await;
    seed_observation(
        pool,
        "item-dec-new",
        "rev-dec-new",
        "snap-1",
        "project_decision",
        "fp-new",
        "proj-alpha",
        "2026-09-15T02:00:00Z",
    )
    .await;

    // 3. 运行 consolidation，模拟 Agent 或准入校验失败
    let consolidation_res = reconcile_project_consolidation(
        pool,
        "default",
        "proj-alpha",
        Some("/path/alpha"),
        &lock_map,
        Some(|_input: ProjectConsolidationInput| async move {
            Err::<ProjectConsolidationResult, _>(AppError::external("Simulated Agent Failure"))
        }),
    )
    .await;

    assert!(
        consolidation_res.is_err(),
        "Consolidation should fail when Agent fails"
    );

    // 4. 验证原有的 current L2 条目保持完好无损 (M35-L2-06: 失败保留 current L2)
    let loaded = load_l2_items(pool, "default", "proj-alpha")
        .await
        .expect("load l2 items after failure");

    assert_eq!(
        loaded.len(),
        1,
        "Original L2 item must be preserved on failure"
    );
    assert_eq!(loaded[0].item_id, existing_l2_item_id);
    assert_eq!(loaded[0].title, "Pre-existing Decision");
}

#[tokio::test]
async fn test_m35_l2_successful_consolidation_and_fingerprint_reuse() {
    let (service, _root) = setup_test_service().await;
    let pool = service.db.pool();
    let lock_map = ProjectConsolidationLockMap::new();

    // 1. 插入一个合格候选
    seed_snapshot(pool, "snap-1", 1, "2026-09-15T02:00:00Z", "generated").await;
    seed_l1_item(
        pool,
        "item-dec-success",
        "rev-dec-success",
        "proj-beta",
        "decision",
        "active",
        "project_decision",
        "Beta Decision Adopted",
        "fp-success",
    )
    .await;
    seed_source_reference(pool, "rev-dec-success", "ref-user-success", true, true).await;
    seed_observation(
        pool,
        "item-dec-success",
        "rev-dec-success",
        "snap-1",
        "project_decision",
        "fp-success",
        "proj-beta",
        "2026-09-15T02:00:00Z",
    )
    .await;

    // 2. 首次运行 Consolidation: 成功提交
    let view1 = reconcile_project_consolidation(
        pool,
        "default",
        "proj-beta",
        Some("/path/beta"),
        &lock_map,
        Some(|input: ProjectConsolidationInput| async move {
            assert_eq!(input.candidates.len(), 1);
            Ok::<_, AppError>(ProjectConsolidationResult {
                operations: vec![ProjectConsolidationOperation::Create {
                    category: MemoryItemCategory::Decision,
                    title: "Beta Decision Adopted".to_string(),
                    statement: "Adopted Beta Strategy".to_string(),
                    rationale: "Validated by benchmark".to_string(),
                    source_refs: vec!["ref-user-success".to_string()],
                }],
            })
        }),
    )
    .await
    .expect("consolidation succeeds")
    .expect("view exists");

    assert_eq!(view1.items.len(), 1);
    assert_eq!(view1.items[0].title, "Beta Decision Adopted");
    let nomination: String = sqlx::query_scalar(
            "SELECT promotion_nomination FROM memory_item_revisions WHERE tenant_id = 'default' AND id = ?1",
        )
        .bind(&view1.items[0].revision_id)
        .fetch_one(pool)
        .await
        .expect("load preserved promotion nomination");
    assert_eq!(nomination, "project_decision");
    assert!(
        !view1.revision_hash.is_empty(),
        "Revision hash must be generated"
    );

    // 3. 再次运行 Consolidation: 由于输入指纹未变，Agent runner 被跳过 (0 Agent calls)
    let view2 = reconcile_project_consolidation(
        pool,
        "default",
        "proj-beta",
        Some("/path/beta"),
        &lock_map,
        Some(|_input: ProjectConsolidationInput| async move {
            panic!("Agent should not be called when input fingerprint is identical!");
            #[allow(unreachable_code)]
            Ok::<_, AppError>(ProjectConsolidationResult { operations: vec![] })
        }),
    )
    .await
    .expect("consolidation with cached fingerprint")
    .expect("view exists");

    assert_eq!(view2.items.len(), 1);
    assert_eq!(view2.revision_hash, view1.revision_hash);
}

#[tokio::test]
async fn test_context_resolver_reads_current_l2_project_memory() {
    let (service, root) = setup_test_service().await;
    let pool = service.db.pool();

    let project_dir = root.join("app");
    fs::create_dir_all(&project_dir).expect("create test project dir");
    let project_path_str = project_dir.to_str().expect("valid utf8 path");

    let now = Utc::now().to_rfc3339();
    let item_id = "item-l2-ctx";
    let rev_id = "rev-l2-ctx";

    let project_key =
        crate::backend::application::recent::resolve_project_directory(project_path_str, &[])
            .unwrap_or_else(|| project_path_str.to_string());

    sqlx::query(
        "INSERT INTO memory_items (\
                tenant_id, id, layer, project_key, current_revision_id, lifecycle, \
                first_seen_at, last_seen_at, created_at, updated_at\
             ) VALUES ('default', ?1, 'l2', ?2, ?3, 'current', ?4, ?4, ?4, ?4)",
    )
    .bind(item_id)
    .bind(&project_key)
    .bind(rev_id)
    .bind(&now)
    .execute(pool)
    .await
    .expect("insert l2 item");

    sqlx::query(
            "INSERT INTO memory_item_revisions (\
                tenant_id, id, item_id, revision_number, category, status, title, \
                summary, rationale, recommendation_rank, promotion_nomination, \
                occurred_at, evidence_fingerprint, generated_by_snapshot_id, \
                supersedes_revision_id, created_at\
             ) VALUES ('default', ?1, ?2, 1, 'decision', 'active', 'Use SQLite WAL Mode', \
                       'Enable WAL mode for performance', 'Benchmark proof', NULL, 'none', ?3, 'fp-wal', NULL, NULL, ?3)",
        )
        .bind(rev_id)
        .bind(item_id)
        .bind(&now)
        .execute(pool)
        .await
        .expect("insert l2 revision");

    let res = service
        .resolve_memory_context(crate::backend::application::MemoryContextResolveParams {
            project_path: Some(project_path_str.to_string()),
            query: None,
            token_budget: Some(2000),
        })
        .await
        .expect("resolve memory context");

    assert!(
        res.text.contains("Use SQLite WAL Mode"),
        "Context text must contain L2 title"
    );
    assert!(
        res.text.contains("Enable WAL mode for performance"),
        "Context text must contain L2 summary"
    );
    assert!(
        res.references
            .iter()
            .any(|r| r.kind == "project_memory_l2" && r.id == rev_id),
        "Context references must include project_memory_l2"
    );
}
