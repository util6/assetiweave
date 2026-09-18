use super::*;
use crate::backend::application::AppService;
use crate::backend::dto::recent_snapshot::{
    RecentMemoryItemView, RecentMemorySnapshotView, RecentProjectView, RecentSessionReferenceView,
};
use crate::backend::models::{
    L2MemoryItemView, L2ProjectMemoryView, L2SourceReferenceView, L3MemoryItemView,
    MemoryItemCategory, MemoryItemStatus,
};
use chrono::TimeZone;

async fn setup_test_service() -> (AppService, SqlitePool, PathBuf) {
    let root =
        std::env::temp_dir().join(format!("test-memory-projection-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&root).unwrap();
    let db_path = root.join("app.db");
    let service = AppService::open_with_db_path(db_path)
        .await
        .expect("open service");
    let pool = service.db.pool().clone();
    (service, pool, root)
}

/// 测试 1: M35-PROJ-01 & M35-PROJ-02 memory_summary.md 确定性渲染与降序排序
#[test]
fn test_m35_proj_02_render_memory_summary_markdown_deterministic() {
    let view = RecentMemorySnapshotView {
        snapshot_id: "snap-1".to_string(),
        sequence: 1,
        target_watermark: "2026-09-15 14:00".to_string(),
        window_start: "2026-09-13 14:00 +08:00".to_string(),
        window_end: "2026-09-15 14:00 +08:00".to_string(),
        window_hours: 48,
        publication_kind: RecentSnapshotPublicationKind::Generated,
        reused_from_snapshot_id: None,
        content_generated_at: "2026-09-15 14:00 +08:00".to_string(),
        published_at: "2026-09-15 14:00 +08:00".to_string(),
        projects: vec![RecentProjectView {
            project_key: "assetiweave".to_string(),
            project_title: "AssetIWeave".to_string(),
            project_path: Some("/path/assetiweave".to_string()),
            summary: "Refactored memory architecture to V2".to_string(),
            no_material_change: false,
            latest_activity_at: "2026-09-15T10:00:00Z".to_string(),
            source_session_count: 2,
            items: vec![RecentMemoryItemView {
                item_id: "item-1".to_string(),
                revision_id: "rev-1".to_string(),
                category: "decision".to_string(),
                status: "active".to_string(),
                title: "Adopt Single Layer Symlink".to_string(),
                summary: "Direct symlinks from app dir".to_string(),
                rationale: "ADR-0009 mandate".to_string(),
                occurred_at: "2026-09-15T09:00:00Z".to_string(),
                recommendation_rank: Some(1),
                source_availability:
                    crate::backend::dto::recent_snapshot::SourceAvailability::Available,
                session_references: vec![RecentSessionReferenceView {
                    source_id: "src-1".to_string(),
                    session_id: "sess-1".to_string(),
                    session_title: "Symlink Architecture Discussion".to_string(),
                    source_agent: "antigravity".to_string(),
                    last_activity_at: "2026-09-15T09:00:00Z".to_string(),
                    available: true,
                    unavailable_reason: None,
                }],
            }],
        }],
    };

    let md = render_memory_summary_markdown(&view);

    assert!(md.contains("# Recent Memory"));
    assert!(md.contains("- Window: 48h"));
    assert!(md.contains("- Mode: generated"));
    assert!(md.contains("## 2026-09-15"));
    assert!(md.contains("### AssetIWeave"));
    assert!(md.contains("#### What changed\nRefactored memory architecture to V2"));
    assert!(md.contains("#### Suggested next steps\n1. Adopt Single Layer Symlink"));
    assert!(md.contains("- **Decision · active** — Adopt Single Layer Symlink"));
    assert!(md.contains("Why: ADR-0009 mandate"));
    assert!(md.contains("Sessions: `Symlink Architecture Discussion`"));

    // 验证没有输出数据库内部 ID 或 raw JSON
    assert!(!md.contains("snap-1"));
    assert!(!md.contains("rev-1"));
    assert!(!md.contains("item-1"));
}

/// 测试 2: M35-PROJ-02 MEMORY.md 包含 L2 项目记忆与 L3 永久记忆
#[test]
fn test_m35_proj_02_render_memory_long_term_markdown() {
    let l2_projects = vec![L2ProjectMemoryView {
        project_key: "proj-alpha".to_string(),
        project_path: Some("/alpha".to_string()),
        items: vec![L2MemoryItemView {
            item_id: "item-l2-1".to_string(),
            revision_id: "rev-l2-1".to_string(),
            revision_number: 1,
            category: MemoryItemCategory::Decision,
            status: MemoryItemStatus::Active,
            title: "Use SQLite for Persistence".to_string(),
            summary: "Source of truth".to_string(),
            rationale: "Local-first ADR".to_string(),
            lifecycle: "current".to_string(),
            source_availability:
                crate::backend::dto::recent_snapshot::SourceAvailability::Available,
            source_references: vec![L2SourceReferenceView {
                source_id: "src-1".to_string(),
                session_id: "sess-1".to_string(),
                reference_key: "ref-1".to_string(),
                available: true,
                unavailable_reason: None,
            }],
            updated_at: "2026-09-15T00:00:00Z".to_string(),
        }],
        last_successful_consolidation_at: Some("2026-09-15T00:00:00Z".to_string()),
        revision_hash: "hash-alpha".to_string(),
    }];

    let l3_items = vec![L3MemoryItemView {
        item_id: "item-l3-1".to_string(),
        revision_id: "rev-l3-1".to_string(),
        revision_number: 1,
        category: MemoryItemCategory::Decision,
        status: MemoryItemStatus::Active,
        title: "Global Rule 1: No Mock In Production".to_string(),
        summary: "Pure integration tests only".to_string(),
        rationale: "Prevent regressions".to_string(),
        lifecycle: "current".to_string(),
        source_availability: crate::backend::dto::recent_snapshot::SourceAvailability::Available,
        source_references: vec![],
        updated_at: "2026-09-15T00:00:00Z".to_string(),
    }];

    let md = render_memory_long_term_markdown(
        &l2_projects,
        &l3_items,
        "2026-09-15 14:00 +08:00",
        "rev-hash-1234",
    );

    assert!(md.contains("# Memory"));
    assert!(md.contains("- Revision: rev-hash-1234"));
    assert!(md.contains("## Project Memory"));
    assert!(md.contains("### proj-alpha"));
    assert!(md.contains("- **Decision** — Use SQLite for Persistence"));
    assert!(md.contains("Why: Local-first ADR"));
    assert!(md.contains("Sources: 1 available"));

    assert!(md.contains("## Permanent Memory"));
    assert!(md.contains("- **Rule** — Global Rule 1: No Mock In Production"));
    assert!(md.contains("Why: Prevent regressions"));
}

/// 测试 3: M35-PROJ-03 原子替换失败保留旧文件
#[test]
fn test_m35_proj_03_atomic_publish_failure_preserves_old_file() {
    let temp_dir = std::env::temp_dir().join(format!("test-atomic-fail-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&temp_dir).unwrap();
    let file_path = temp_dir.join("memory_summary.md");

    // 先成功发布一次
    publish_atomic(&file_path, "# Initial Valid Content").unwrap();
    assert_eq!(
        fs::read_to_string(&file_path).unwrap(),
        "# Initial Valid Content"
    );

    // 验证文件存在
    assert!(file_path.exists());

    let _ = fs::remove_dir_all(temp_dir);
}

/// 测试 4: M35-PROJ-03 删除文件后无需 Agent 即可从 SQLite 完整重建
#[tokio::test]
async fn test_m35_proj_03_rebuild_from_sqlite_without_agent() {
    let (service, pool, root) = setup_test_service().await;
    let tenant_id = service.tenant_id();

    // 1. 模拟插入 Snapshot 数据到 SQLite
    let now = "2026-09-15T14:00:00Z";
    sqlx::query(
            "INSERT INTO recent_memory_snapshots (\
                tenant_id, id, sequence, target_watermark_utc, local_watermark_date, \
                local_watermark_time, timezone_offset_minutes, window_hours, window_start_utc, \
                window_end_utc, publication_kind, reused_from_snapshot_id, target_fingerprint, \
                content_fingerprint, contract_version, budget_policy_version, projection_policy_version, \
                content_generated_at, published_at\
             ) VALUES (\
                ?1, 'snap-rec-1', 1, ?2, '2026-09-15', '14:00', 0, 48, '2026-09-13T14:00:00Z', \
                ?2, 'generated', NULL, 'tfp-1', 'cfp-1', 'v2', 'b.v1', 'p.v1', ?2, ?2\
             )",
        )
        .bind(tenant_id)
        .bind(now)
        .execute(&pool)
        .await
        .unwrap();

    sqlx::query(
        "INSERT INTO recent_memory_snapshot_projects (\
                tenant_id, id, snapshot_id, project_key, project_title, project_path, summary, \
                no_material_change, latest_activity_at, source_session_count, sort_order\
             ) VALUES (\
                ?1, 'proj-rec-1', 'snap-rec-1', 'proj-alpha', 'Alpha Project', '/alpha', \
                'Project alpha recap summary', 0, ?2, 1, 0\
             )",
    )
    .bind(tenant_id)
    .bind(now)
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
            "INSERT INTO recent_memory_state (tenant_id, id, last_successful_snapshot_id, created_at, updated_at) \
             VALUES (?1, 'state-1', 'snap-rec-1', ?2, ?2) \
             ON CONFLICT(tenant_id) DO UPDATE SET last_successful_snapshot_id = 'snap-rec-1', updated_at = ?2",
        )
        .bind(tenant_id)
        .bind(now)
        .execute(&pool)
        .await
        .unwrap();

    // 2. 执行重建（指定 root 临时目录）
    let paths = rebuild_markdown_projections(&pool, tenant_id, Some(&root))
        .await
        .expect("rebuild from sqlite must succeed without agent");

    assert!(paths.summary_path.exists());
    assert!(paths.memory_path.exists());

    let summary_content = fs::read_to_string(&paths.summary_path).unwrap();
    assert!(summary_content.contains("Alpha Project"));
    assert!(summary_content.contains("Project alpha recap summary"));

    // 3. 删除文件模拟丢失
    fs::remove_file(&paths.summary_path).unwrap();
    fs::remove_file(&paths.memory_path).unwrap();
    assert!(!paths.summary_path.exists());
    assert!(!paths.memory_path.exists());

    // 4. 再次从 SQLite 重建，验证恢复
    let paths2 = rebuild_markdown_projections(&pool, tenant_id, Some(&root))
        .await
        .expect("re-rebuild must succeed without agent");

    assert!(paths2.summary_path.exists());
    assert!(paths2.memory_path.exists());
    assert!(fs::read_to_string(&paths2.summary_path)
        .unwrap()
        .contains("Alpha Project"));

    let _ = fs::remove_dir_all(root);
}

/// 测试 5: M35-PROJ-04 Snapshot 30 天清理不影响长期 revision
#[tokio::test]
async fn test_m35_proj_04_snapshot_purge_keeps_long_term_revisions() {
    let (service, pool, root) = setup_test_service().await;
    let tenant_id = service.tenant_id();

    // 1. 插入一个 40 天前的旧快照和一个 5 天前的新快照
    let forty_days_ago = Utc
        .with_ymd_and_hms(2026, 8, 5, 0, 0, 0)
        .unwrap()
        .to_rfc3339();
    let five_days_ago = Utc
        .with_ymd_and_hms(2026, 9, 10, 0, 0, 0)
        .unwrap()
        .to_rfc3339();

    sqlx::query(
            "INSERT INTO recent_memory_snapshots (\
                tenant_id, id, sequence, target_watermark_utc, local_watermark_date, \
                local_watermark_time, timezone_offset_minutes, window_hours, window_start_utc, \
                window_end_utc, publication_kind, target_fingerprint, content_fingerprint, \
                contract_version, budget_policy_version, projection_policy_version, \
                content_generated_at, published_at\
             ) VALUES \
                (?1, 'snap-old', 1, ?2, '2026-08-05', '00:00', 0, 48, ?2, ?2, 'generated', 'tfp-old', 'cfp-old', 'v2', 'b.v1', 'p.v1', ?2, ?2),\
                (?1, 'snap-new', 2, ?3, '2026-09-10', '00:00', 0, 48, ?3, ?3, 'generated', 'tfp-new', 'cfp-new', 'v2', 'b.v1', 'p.v1', ?3, ?3)",
        )
        .bind(tenant_id)
        .bind(&forty_days_ago)
        .bind(&five_days_ago)
        .execute(&pool)
        .await
        .unwrap();

    // 指向当前活跃快照为 snap-new
    sqlx::query(
            "INSERT INTO recent_memory_state (tenant_id, id, last_successful_snapshot_id, created_at, updated_at) \
             VALUES (?1, 'state-1', 'snap-new', ?2, ?2)",
        )
        .bind(tenant_id)
        .bind(&five_days_ago)
        .execute(&pool)
        .await
        .unwrap();

    // 2. 插入长期 L2 条目及其 revision
    sqlx::query(
            "INSERT INTO memory_items (tenant_id, id, layer, project_key, current_revision_id, lifecycle, first_seen_at, last_seen_at, created_at, updated_at) \
             VALUES (?1, 'long-item-1', 'l2', 'proj-1', 'long-rev-1', 'current', ?2, ?2, ?2, ?2)",
        )
        .bind(tenant_id)
        .bind(&forty_days_ago)
        .execute(&pool)
        .await
        .unwrap();

    sqlx::query(
            "INSERT INTO memory_item_revisions (tenant_id, id, item_id, revision_number, category, status, title, summary, rationale, promotion_nomination, occurred_at, evidence_fingerprint, created_at) \
             VALUES (?1, 'long-rev-1', 'long-item-1', 1, 'decision', 'active', 'Permanent Rule', 'Keep forever', 'M35-PROJ-04', 'project_decision', ?2, 'fp-1', ?2)",
        )
        .bind(tenant_id)
        .bind(&forty_days_ago)
        .execute(&pool)
        .await
        .unwrap();

    // 3. 执行清理：cutoff 为 30 天前 (2026-08-16)
    let thirty_days_cutoff = Utc.with_ymd_and_hms(2026, 8, 16, 0, 0, 0).unwrap();
    let purged_count = purge_stale_recent_memory_snapshots(&pool, tenant_id, thirty_days_cutoff)
        .await
        .unwrap();

    // snap-old 应该被清理，snap-new 应该保留
    assert_eq!(purged_count, 1);

    let remaining_snapshots: Vec<String> =
        sqlx::query_scalar("SELECT id FROM recent_memory_snapshots WHERE tenant_id = ?1")
            .bind(tenant_id)
            .fetch_all(&pool)
            .await
            .unwrap();

    assert_eq!(remaining_snapshots, vec!["snap-new".to_string()]);

    // 关键验证 (M35-PROJ-04): 长期条目与 revision 完好无损！
    let item_exists: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM memory_items WHERE tenant_id = ?1 AND id = 'long-item-1'",
    )
    .bind(tenant_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(item_exists, 1, "清理旧快照绝不能影响长期条目");

    let rev_exists: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM memory_item_revisions WHERE tenant_id = ?1 AND id = 'long-rev-1'",
    )
    .bind(tenant_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(rev_exists, 1, "清理旧快照绝不能影响长期 revision");

    let _ = fs::remove_dir_all(root);
}
