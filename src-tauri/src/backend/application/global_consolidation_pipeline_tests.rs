use super::*;
use crate::backend::application::AppService;
use chrono::TimeZone;

async fn setup_test_db() -> (AppService, SqlitePool, std::path::PathBuf) {
    let root = std::env::temp_dir().join(format!(
        "test-global-consolidation-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&root).unwrap();
    let db_path = root.join("app.db");
    let service = AppService::open_with_db_path(db_path)
        .await
        .expect("open service");
    let pool = service.db.pool().clone();
    (service, pool, root)
}

/// 辅助插入 L2 条目及其 revision 与引用
async fn insert_l2_fixture(
    pool: &SqlitePool,
    tenant_id: &str,
    project_key: &str,
    title: &str,
    summary: &str,
    category: &str,
    nomination: &str,
    session_id: &str,
    reference_key: &str,
    available: bool,
) -> (String, String) {
    let now = "2026-09-15T00:00:00Z";
    let item_id = format!("l2-item-{}", uuid::Uuid::new_v4());
    let rev_id = format!("l2-rev-{}", uuid::Uuid::new_v4());

    sqlx::query(
        "INSERT INTO memory_items \
             (tenant_id, id, layer, project_key, current_revision_id, lifecycle, \
              first_seen_at, last_seen_at, created_at, updated_at) \
             VALUES (?1, ?2, 'l2', ?3, ?4, 'current', ?5, ?5, ?5, ?5)",
    )
    .bind(tenant_id)
    .bind(&item_id)
    .bind(project_key)
    .bind(&rev_id)
    .bind(now)
    .execute(pool)
    .await
    .expect("insert l2 item");

    let mut hasher = Sha256::new();
    hasher.update(title.as_bytes());
    let fp = format!("{:x}", hasher.finalize());

    sqlx::query(
        "INSERT INTO memory_item_revisions \
             (tenant_id, id, item_id, revision_number, category, status, title, summary, \
              rationale, recommendation_rank, promotion_nomination, occurred_at, \
              evidence_fingerprint, created_at) \
             VALUES (?1, ?2, ?3, 1, ?4, 'active', ?5, ?6, 'Test rationale', NULL, ?7, ?8, ?9, ?8)",
    )
    .bind(tenant_id)
    .bind(&rev_id)
    .bind(&item_id)
    .bind(category)
    .bind(title)
    .bind(summary)
    .bind(nomination)
    .bind(now)
    .bind(&fp)
    .execute(pool)
    .await
    .expect("insert l2 revision");

    let ref_id = format!("ref-{}", uuid::Uuid::new_v4());
    let avail_str = if available {
        "available"
    } else {
        "unavailable"
    };
    let reason = if available { None } else { Some("deleted") };

    sqlx::query(
            "INSERT INTO memory_item_source_references \
             (tenant_id, id, item_revision_id, record_kind, source_id, session_id, \
              question_id, reference_key, source_revision, availability, unavailable_reason, created_at) \
             VALUES (?1, ?2, ?3, 'session', 'src-1', ?4, ?5, ?6, 1, ?7, ?8, ?9)",
        )
        .bind(tenant_id)
        .bind(&ref_id)
        .bind(&rev_id)
        .bind(session_id)
        .bind(format!("question-{session_id}"))
        .bind(reference_key)
        .bind(avail_str)
        .bind(reason)
        .bind(now)
        .execute(pool)
        .await
        .expect("insert l2 ref");

    (item_id, rev_id)
}

/// 测试 1: M35-L3-02 跨项目候选必须由至少两个不同真实 project_key 独立支持
#[tokio::test]
async fn test_m35_l3_02_cross_project_candidate_requires_two_distinct_real_projects() {
    let (_service, pool, root) = setup_test_db().await;

    // 项目 A 引入知识 "Shared Error Handling Pattern"
    insert_l2_fixture(
        &pool,
        "default",
        "project-alpha",
        "Shared Error Handling Pattern",
        "Always wrap external calls in AppError::external",
        "decision",
        "cross_project_pattern",
        "session-alpha-1",
        "ref-alpha-1",
        true,
    )
    .await;

    // 单项目时评估候选: 不足以成为 cross_project_pattern 候选
    let candidates_one = evaluate_l3_candidates(&pool, "default").await.unwrap();
    assert!(
        candidates_one.is_empty(),
        "单项目支持不能形成 cross_project_pattern 候选"
    );

    // 如果第二个项目来自 unassigned，仍然不能成为合格候选 (M35-L3-02)
    insert_l2_fixture(
        &pool,
        "default",
        "unassigned",
        "Shared Error Handling Pattern",
        "Always wrap external calls in AppError::external",
        "decision",
        "cross_project_pattern",
        "session-unassigned-1",
        "ref-unassigned-1",
        true,
    )
    .await;
    let candidates_unassigned = evaluate_l3_candidates(&pool, "default").await.unwrap();
    assert!(
        candidates_unassigned.is_empty(),
        "unassigned 项目不能作为独立支持项目"
    );

    // 项目 B (第二个真实项目) 独立支持该知识
    insert_l2_fixture(
        &pool,
        "default",
        "project-beta",
        "Shared Error Handling Pattern",
        "Always wrap external calls in AppError::external",
        "decision",
        "cross_project_pattern",
        "session-beta-1",
        "ref-beta-1",
        true,
    )
    .await;

    // 两个真实独立项目均支持: 形成合格候选
    let candidates_two = evaluate_l3_candidates(&pool, "default").await.unwrap();
    assert_eq!(candidates_two.len(), 1);
    assert_eq!(candidates_two[0].title, "Shared Error Handling Pattern");
    assert_eq!(
        candidates_two[0].nomination,
        MemoryPromotionNomination::CrossProjectPattern
    );
    assert_eq!(candidates_two[0].supporting_project_keys.len(), 2);
    assert!(candidates_two[0]
        .supporting_project_keys
        .contains(&"project-alpha".to_string()));
    assert!(candidates_two[0]
        .supporting_project_keys
        .contains(&"project-beta".to_string()));

    let _ = std::fs::remove_dir_all(root);
}

/// 测试 2: M35-L3-02 & M35-L3-04 unavailable 引用不计入独立支持数
#[tokio::test]
async fn test_m35_l3_02_unavailable_references_do_not_count_towards_cross_project() {
    let (_service, pool, root) = setup_test_db().await;

    // 项目 A: available 引用
    insert_l2_fixture(
        &pool,
        "default",
        "project-alpha",
        "Universal Metric Format",
        "Use ISO timestamps for all metrics",
        "decision",
        "cross_project_pattern",
        "session-alpha-1",
        "ref-alpha-1",
        true,
    )
    .await;

    // 项目 B: unavailable 引用 (来源已失效)
    insert_l2_fixture(
        &pool,
        "default",
        "project-beta",
        "Universal Metric Format",
        "Use ISO timestamps for all metrics",
        "decision",
        "cross_project_pattern",
        "session-beta-1",
        "ref-beta-1",
        false, // unavailable!
    )
    .await;

    // 因为项目 B 仅有 unavailable 引用，不计入独立支持
    let candidates = evaluate_l3_candidates(&pool, "default").await.unwrap();
    assert!(
        candidates.is_empty(),
        "包含 unavailable 引用的项目不能计入独立支持数"
    );

    let _ = std::fs::remove_dir_all(root);
}

/// 测试 3: M35-L3-02 明确全局规则 (global_rule) 需 available 用户引用
#[tokio::test]
async fn test_m35_l3_02_global_rule_requires_available_reference() {
    let (_service, pool, root) = setup_test_db().await;

    // 仅在单项目中声明，但被提名为 global_rule
    insert_l2_fixture(
        &pool,
        "default",
        "project-alpha",
        "Global License Policy",
        "All internal libraries must use Apache-2.0",
        "decision",
        "global_rule",
        "session-alpha-1",
        "ref-alpha-1",
        true,
    )
    .await;

    let candidates = evaluate_l3_candidates(&pool, "default").await.unwrap();
    assert_eq!(candidates.len(), 1);
    assert_eq!(
        candidates[0].nomination,
        MemoryPromotionNomination::GlobalRule
    );
    assert_eq!(candidates[0].title, "Global License Policy");

    let _ = std::fs::remove_dir_all(root);
}

/// 测试 4: M35-L3-03 0 候选绝不触发 Agent
#[tokio::test]
async fn test_m35_l3_03_zero_candidates_triggers_zero_agent_calls() {
    let (_service, pool, root) = setup_test_db().await;

    let now = Utc.with_ymd_and_hms(2026, 9, 15, 0, 0, 0).unwrap();
    let should_trigger = should_trigger_global_consolidation(&pool, "default", 0, now, false)
        .await
        .unwrap();
    assert!(!should_trigger, "0 候选时 should_trigger 必须为 false");

    // 即使是 manual_rebuild，0 候选也不能触发
    let should_trigger_manual = should_trigger_global_consolidation(&pool, "default", 0, now, true)
        .await
        .unwrap();
    assert!(
        !should_trigger_manual,
        "0 候选时 manual_rebuild 依然必须为 false"
    );

    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test]
async fn test_memory_v2_frozen_global_input_rejects_changed_sqlite_evidence() {
    let (_service, pool, root) = setup_test_db().await;
    insert_l2_fixture(
        &pool,
        "default",
        "project-frozen",
        "Frozen global rule",
        "Initial statement",
        "decision",
        "global_rule",
        "session-frozen",
        "ref-frozen",
        true,
    )
    .await;

    let frozen_input = load_global_consolidation_input(&pool, "default")
        .await
        .expect("freeze global input");
    let revision_id: String = sqlx::query_scalar(
        "SELECT current_revision_id FROM memory_items \
             WHERE tenant_id = 'default' AND project_key = 'project-frozen'",
    )
    .fetch_one(&pool)
    .await
    .expect("find revision");
    sqlx::query(
        "UPDATE memory_item_revisions SET summary = 'changed after enqueue' \
             WHERE tenant_id = 'default' AND id = ?1",
    )
    .bind(&revision_id)
    .execute(&pool)
    .await
    .expect("change source evidence");

    let now = Utc.with_ymd_and_hms(2026, 9, 16, 0, 0, 0).unwrap();
    let error = reconcile_global_consolidation_with_runner_and_lease(
        &pool,
        "default",
        now,
        true,
        None,
        Some(|_| async move {
            panic!("stale frozen input must not call the Agent");
            #[allow(unreachable_code)]
            Ok::<_, AppError>(GlobalConsolidationResult { operations: vec![] })
        }),
        None,
        Some(frozen_input),
    )
    .await
    .expect_err("changed evidence must mark the Work Order stale");

    assert_eq!(error.code(), "MEMORY_WORK_ORDER_STALE");
    let _ = std::fs::remove_dir_all(root);
}

/// 测试 5: M35-L3-03 低频触发（周级/8候选）与指纹跳过
#[tokio::test]
async fn test_m35_l3_03_low_frequency_trigger_and_fingerprint_skip() {
    let (_service, pool, root) = setup_test_db().await;

    let now = Utc.with_ymd_and_hms(2026, 9, 15, 0, 0, 0).unwrap();

    // 少量候选 (3 个)
    let trigger_few = should_trigger_global_consolidation(&pool, "default", 3, now, false)
        .await
        .unwrap();
    // 因为没有上次成功记录，首次有候选允许触发
    assert!(trigger_few);

    // 插入上次成功记录为 3 天前
    let three_days_ago = Utc
        .with_ymd_and_hms(2026, 9, 12, 0, 0, 0)
        .unwrap()
        .to_rfc3339();
    sqlx::query(
            "INSERT INTO global_memory_state (tenant_id, last_successful_consolidation_at, last_input_fingerprint, revision_hash, created_at, updated_at) \
             VALUES ('default', ?1, 'fp-old', 'rev-hash', ?1, ?1)",
        )
        .bind(&three_days_ago)
        .execute(&pool)
        .await
        .unwrap();

    // 3 天前且只有 3 个候选 -> 不足 7 天且未达 8 候选，不触发
    let trigger_blocked = should_trigger_global_consolidation(&pool, "default", 3, now, false)
        .await
        .unwrap();
    assert!(
        !trigger_blocked,
        "未达 7 天且候选少于 8 时不触发低频 Consolidation"
    );

    // 候选达到 8 个 -> 立即触发
    let trigger_threshold = should_trigger_global_consolidation(&pool, "default", 8, now, false)
        .await
        .unwrap();
    assert!(trigger_threshold, "候选达 8 个时立即触发");

    // 超过 7 天 (如 8 天前) -> 触发周级巩固
    let eight_days_ago = Utc
        .with_ymd_and_hms(2026, 9, 7, 0, 0, 0)
        .unwrap()
        .to_rfc3339();
    sqlx::query("UPDATE global_memory_state SET last_successful_consolidation_at = ?1 WHERE tenant_id = 'default'")
            .bind(&eight_days_ago)
            .execute(&pool)
            .await
            .unwrap();

    let trigger_weekly = should_trigger_global_consolidation(&pool, "default", 1, now, false)
        .await
        .unwrap();
    assert!(
        trigger_weekly,
        "距离上次成功超过 7 天且有候选时触发周级维护"
    );

    let _ = std::fs::remove_dir_all(root);
}

/// 测试 6: M35-L3-04 来源失效更新引用可用性，保留 L2/L3 长期条目
#[tokio::test]
async fn test_m35_l3_04_source_invalidation_updates_reference_keeps_l2_l3() {
    let (_service, pool, root) = setup_test_db().await;

    // 创建 source 和 session 实体
    sqlx::query("INSERT INTO conversation_sources (tenant_id, id, adapter_id, name, kind, location, enabled, created_at, updated_at) VALUES ('default', 'src-1', 'adp-1', 'Source 1', 'fs', '/path', 1, '2026-09-15T00:00:00Z', '2026-09-15T00:00:00Z')")
            .execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO conversation_sessions (tenant_id, id, source_id, adapter_id, external_id, title, missing, created_at, imported_at) VALUES ('default', 'sess-1', 'src-1', 'adp-1', 'ext-1', 'Session 1', 0, '2026-09-15T00:00:00Z', '2026-09-15T00:00:00Z')")
            .execute(&pool).await.unwrap();

    // 插入 L2 条目
    let (l2_item_id, _) = insert_l2_fixture(
        &pool,
        "default",
        "proj-a",
        "Immutable Architecture Principle",
        "Always keep domain models decoupled from persistence",
        "decision",
        "project_decision",
        "sess-1",
        "ref-1",
        true,
    )
    .await;

    // 插入 L3 条目
    let now = Utc.with_ymd_and_hms(2026, 9, 15, 0, 0, 0).unwrap();
    let ops = vec![GlobalConsolidationOperation::Create {
        category: MemoryItemCategory::Decision,
        title: "Global Resilience Rule".to_string(),
        statement: "Never drop long term knowledge on source deletion".to_string(),
        rationale: "Contract M35-L3-04".to_string(),
        source_refs: vec!["ref-l3-sess-1".to_string()],
    }];
    let l3_view = reconcile_global_consolidation(&pool, "default", now, true, Some(ops))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(l3_view.items.len(), 1);
    let l3_item_id = l3_view.items[0].item_id.clone();

    // 将该 session 标记为 missing = 1
    sqlx::query("UPDATE conversation_sessions SET missing = 1 WHERE tenant_id = 'default' AND id = 'sess-1'")
            .execute(&pool).await.unwrap();

    // 执行来源失效协调
    reconcile_source_invalidation(&pool, "default", now, &[], &[])
        .await
        .unwrap();

    // 验证引用的状态变为 unavailable, reason = missing
    let l2_ref_avail: String = sqlx::query_scalar(
        "SELECT availability FROM memory_item_source_references WHERE session_id = 'sess-1'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(l2_ref_avail, "unavailable");

    // 关键验证 (M35-L3-04): L2 和 L3 条目的 lifecycle 仍然是 'current'！
    let l2_lifecycle: String = sqlx::query_scalar(
        "SELECT lifecycle FROM memory_items WHERE tenant_id = 'default' AND id = ?1",
    )
    .bind(&l2_item_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        l2_lifecycle, "current",
        "已晋升的 L2 条目不能因来源失效而被删除或 retired"
    );

    let l3_lifecycle: String = sqlx::query_scalar(
        "SELECT lifecycle FROM memory_items WHERE tenant_id = 'default' AND id = ?1",
    )
    .bind(&l3_item_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        l3_lifecycle, "current",
        "已晋升的 L3 条目不能因来源失效而被删除或 retired"
    );

    let _ = std::fs::remove_dir_all(root);
}

/// 测试 7: M35-L3-05 Revise 生成新 revision 并完整保留历史
#[tokio::test]
async fn test_m35_l3_05_revise_creates_new_revision_preserving_history() {
    let (_service, pool, root) = setup_test_db().await;
    let now = Utc.with_ymd_and_hms(2026, 9, 15, 0, 0, 0).unwrap();

    // 1. Create L3 条目
    let create_op = vec![GlobalConsolidationOperation::Create {
        category: MemoryItemCategory::Decision,
        title: "Database Lock Standard".to_string(),
        statement: "Use 500ms lock timeout".to_string(),
        rationale: "Initial policy".to_string(),
        source_refs: vec!["ref-1".to_string()],
    }];
    let view1 = reconcile_global_consolidation(&pool, "default", now, true, Some(create_op))
        .await
        .unwrap()
        .unwrap();
    let item_id = view1.items[0].item_id.clone();
    let rev1_id = view1.items[0].revision_id.clone();
    assert_eq!(view1.items[0].revision_number, 1);
    assert_eq!(view1.items[0].summary, "Use 500ms lock timeout");

    // 2. Revise 文本更新
    let revise_now = Utc.with_ymd_and_hms(2026, 9, 16, 0, 0, 0).unwrap();
    let revise_op = vec![GlobalConsolidationOperation::Revise {
        item_id: item_id.clone(),
        statement: "Use 1000ms lock timeout for large batches".to_string(),
        rationale: "Observed batch timeout under high concurrency".to_string(),
        source_refs: vec!["ref-2".to_string()],
    }];
    let view2 = reconcile_global_consolidation(&pool, "default", revise_now, true, Some(revise_op))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(view2.items.len(), 1);
    assert_eq!(view2.items[0].item_id, item_id);
    assert_eq!(view2.items[0].revision_number, 2);
    assert_eq!(
        view2.items[0].summary,
        "Use 1000ms lock timeout for large batches"
    );
    let rev2_id = view2.items[0].revision_id.clone();
    assert_ne!(rev1_id, rev2_id);

    // 3. 验证历史完整保留 (M35-L3-05)
    let total_revs: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM memory_item_revisions WHERE tenant_id = 'default' AND item_id = ?1",
    )
    .bind(&item_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(total_revs, 2, "旧 revision 必须保留在数据库中供审计与回忆");

    let supersedes_check: Option<String> = sqlx::query_scalar(
        "SELECT supersedes_revision_id FROM memory_item_revisions WHERE id = ?1",
    )
    .bind(&rev2_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(supersedes_check, Some(rev1_id));

    let _ = std::fs::remove_dir_all(root);
}

/// 测试 8: M35-L3-05 Supersede 标记旧条目为 superseded 并新建取代条目
#[tokio::test]
async fn test_m35_l3_05_supersede_marks_old_and_creates_new_l3() {
    let (_service, pool, root) = setup_test_db().await;
    let now = Utc.with_ymd_and_hms(2026, 9, 15, 0, 0, 0).unwrap();

    // 1. 创建旧条目
    let create_op = vec![GlobalConsolidationOperation::Create {
        category: MemoryItemCategory::Decision,
        title: "Legacy State Machine V1".to_string(),
        statement: "Use monolithic state".to_string(),
        rationale: "Early MVP".to_string(),
        source_refs: vec!["ref-1".to_string()],
    }];
    let view1 = reconcile_global_consolidation(&pool, "default", now, true, Some(create_op))
        .await
        .unwrap()
        .unwrap();
    let old_item_id = view1.items[0].item_id.clone();

    // 2. 执行 Supersede 操作
    let supersede_now = Utc.with_ymd_and_hms(2026, 9, 16, 0, 0, 0).unwrap();
    let supersede_op = vec![GlobalConsolidationOperation::Supersede {
        old_item_id: old_item_id.clone(),
        replacement_title: "Event-Driven State Architecture V2".to_string(),
        replacement_statement: "Transition to distributed event-driven state machine".to_string(),
        rationale: "ADR-0015 full replacement".to_string(),
        category: MemoryItemCategory::Decision,
        source_refs: vec!["ref-2".to_string()],
    }];
    let view2 =
        reconcile_global_consolidation(&pool, "default", supersede_now, true, Some(supersede_op))
            .await
            .unwrap()
            .unwrap();

    // 活跃列表中旧条目已被过滤，只包含新条目 (M35-L3-06)
    assert_eq!(view2.items.len(), 1);
    assert_ne!(view2.items[0].item_id, old_item_id);
    assert_eq!(view2.items[0].title, "Event-Driven State Architecture V2");

    // 验证数据库中旧条目 lifecycle 为 superseded
    let old_lifecycle: String =
        sqlx::query_scalar("SELECT lifecycle FROM memory_items WHERE id = ?1")
            .bind(&old_item_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(old_lifecycle, "superseded");

    // 验证 memory_item_supersessions 关系表记录
    let super_row = sqlx::query(
        "SELECT superseded_item_id, superseding_item_id, reason \
             FROM memory_item_supersessions WHERE superseded_item_id = ?1",
    )
    .bind(&old_item_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    let target_item_id: String = super_row.get("superseding_item_id");
    assert_eq!(target_item_id, view2.items[0].item_id);
    let reason: String = super_row.get("reason");
    assert_eq!(reason, "ADR-0015 full replacement");

    let _ = std::fs::remove_dir_all(root);
}

/// 测试 9: M35-L3-06 Context Resolver 读取当前 L3 且忽略 superseded
#[tokio::test]
async fn test_m35_l3_06_context_resolver_reads_current_l3_and_ignores_superseded() {
    let (service, pool, root) = setup_test_db().await;
    let now = Utc.with_ymd_and_hms(2026, 9, 15, 0, 0, 0).unwrap();

    // 创建条目 A 并随后 supersede 它
    let ops1 = vec![GlobalConsolidationOperation::Create {
        category: MemoryItemCategory::Decision,
        title: "Old Global Rule".to_string(),
        statement: "Old deprecated statement".to_string(),
        rationale: "To be superseded".to_string(),
        source_refs: vec!["ref-old".to_string()],
    }];
    let view1 = reconcile_global_consolidation(&pool, "default", now, true, Some(ops1))
        .await
        .unwrap()
        .unwrap();
    let old_item_id = view1.items[0].item_id.clone();

    let ops2 = vec![GlobalConsolidationOperation::Supersede {
        old_item_id: old_item_id.clone(),
        replacement_title: "Active Global Rule".to_string(),
        replacement_statement: "New active long-term principle".to_string(),
        rationale: "Replaces old rule".to_string(),
        category: MemoryItemCategory::Decision,
        source_refs: vec!["ref-new".to_string()],
    }];
    reconcile_global_consolidation(&pool, "default", now, true, Some(ops2))
        .await
        .unwrap();

    // Context 解析
    let ctx = service
        .resolve_memory_context(crate::backend::application::MemoryContextResolveParams {
            project_path: None,
            query: None,
            token_budget: Some(2000),
        })
        .await
        .unwrap();

    assert!(ctx.text.contains("Active Global Rule"));
    assert!(ctx.text.contains("New active long-term principle"));
    assert!(!ctx.text.contains("Old Global Rule"));
    assert!(!ctx.text.contains("Old deprecated statement"));

    // 验证 references 包含 global_memory_l3
    let l3_refs: Vec<_> = ctx
        .references
        .iter()
        .filter(|r| r.kind == "global_memory_l3")
        .collect();
    assert_eq!(l3_refs.len(), 1);

    let _ = std::fs::remove_dir_all(root);
}
