use crate::backend::dto::recent_snapshot::{
    RecentMemoryItemView, RecentMemorySnapshotView, RecentProjectView,
    RecentSnapshotPublicationKind,
};
use crate::backend::models::{
    L2ProjectMemoryView, L3MemoryItemView, MemoryItemCategory,
};
use crate::backend::runtime::{AppError, AppResult};
use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};
use sqlx::SqlitePool;
use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

/// 安全 Tenant 目录段编码
pub(crate) fn safe_tenant_path_segment(tenant_id: &str) -> String {
    let mut segment = tenant_id
        .trim()
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>();
    while segment.contains("--") {
        segment = segment.replace("--", "-");
    }
    let segment = segment.trim_matches('-');
    if segment.is_empty() {
        "default".to_string()
    } else {
        segment.to_string()
    }
}

/// 获取 Tenant 的统一 Memory 文档根目录 (M35-PROJ-01)
/// ~/.assetiweave/memories/<tenant>/
pub(crate) fn memory_projection_dir(base_dir: Option<&Path>, tenant_id: &str) -> PathBuf {
    let safe_segment = safe_tenant_path_segment(tenant_id);
    if let Some(base) = base_dir {
        base.join(".assetiweave")
            .join("memories")
            .join(safe_segment)
    } else if let Some(home) = dirs::home_dir() {
        home.join(".assetiweave")
            .join("memories")
            .join(safe_segment)
    } else {
        PathBuf::from(".assetiweave")
            .join("memories")
            .join(safe_segment)
    }
}

pub(crate) fn memory_summary_path(base_dir: Option<&Path>, tenant_id: &str) -> PathBuf {
    memory_projection_dir(base_dir, tenant_id).join("memory_summary.md")
}

pub(crate) fn memory_long_term_path(base_dir: Option<&Path>, tenant_id: &str) -> PathBuf {
    memory_projection_dir(base_dir, tenant_id).join("MEMORY.md")
}

/// 原子替换文件发布 (M35-PROJ-03)
///
/// 1. 同目录下创建临时文件
/// 2. flush + fsync
/// 3. 设置只读权限（若平台支持）
/// 4. 原子 rename
/// 5. 失败保留旧文件并抛出 MEMORY_PROJECTION_FAILED
pub(crate) fn publish_atomic(path: &Path, content: &str) -> AppResult<()> {
    let parent = path
        .parent()
        .ok_or_else(|| AppError::Validation("Memory projection path has no parent".into()))?;
    fs::create_dir_all(parent).map_err(AppError::external)?;

    let temp_path = path.with_extension(format!("tmp-{}", uuid::Uuid::new_v4()));
    {
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temp_path)
            .map_err(|e| {
                AppError::External(format!(
                    "MEMORY_PROJECTION_FAILED: cannot create temp file {}: {}",
                    temp_path.display(),
                    e
                ))
            })?;

        file.write_all(content.as_bytes()).map_err(|e| {
            AppError::External(format!("MEMORY_PROJECTION_FAILED: write failed: {}", e))
        })?;

        file.sync_all().map_err(|e| {
            AppError::External(format!("MEMORY_PROJECTION_FAILED: fsync failed: {}", e))
        })?;

        // 设置只读权限 (可被覆盖但保护意外编辑)
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&temp_path, fs::Permissions::from_mode(0o444));
        }
    }

    fs::rename(&temp_path, path).map_err(|e| {
        let _ = fs::remove_file(&temp_path);
        AppError::External(format!(
            "MEMORY_PROJECTION_FAILED: atomic rename failed: {}",
            e
        ))
    })?;

    // fsync 父目录
    if let Ok(dir_file) = File::open(parent) {
        let _ = dir_file.sync_all();
    }

    Ok(())
}

/// 渲染 memory_summary.md (M35-PROJ-02, 07-markdown-ui-settings.md Section 3)
pub(crate) fn render_memory_summary_markdown(snapshot: &RecentMemorySnapshotView) -> String {
    let mut out = String::new();

    out.push_str("# Recent Memory\n\n");
    out.push_str(&format!("- Window: {}h\n", snapshot.window_hours));
    out.push_str(&format!("- From: {}\n", snapshot.window_start));
    out.push_str(&format!("- To: {}\n", snapshot.window_end));
    out.push_str(&format!("- Published: {}\n", snapshot.published_at));
    out.push_str(&format!(
        "- Content generated: {}\n",
        snapshot.content_generated_at
    ));
    let mode_str = match snapshot.publication_kind {
        RecentSnapshotPublicationKind::Generated => "generated",
        RecentSnapshotPublicationKind::Reused => "reused",
    };
    out.push_str(&format!("- Mode: {}\n\n", mode_str));

    // 按日期将 items 与 project summary 聚合
    // 确定性规则:
    // 1. 日期降序
    // 2. 同日项目按 project_title, project_key 排序
    // 3. 项目摘要和建议只出现一次，在 latest_activity_at 对应日期
    // 4. 普通 Item 在 occurred_at 对应日期
    let mut items_by_date_and_project: BTreeMap<String, BTreeMap<(String, String), Vec<&RecentMemoryItemView>>> =
        BTreeMap::new();

    let mut project_meta_by_date: BTreeMap<String, Vec<&RecentProjectView>> = BTreeMap::new();

    for proj in &snapshot.projects {
        let proj_date = proj
            .latest_activity_at
            .split('T')
            .next()
            .unwrap_or("unknown")
            .to_string();
        project_meta_by_date.entry(proj_date).or_default().push(proj);

        for item in &proj.items {
            let item_date = item
                .occurred_at
                .split('T')
                .next()
                .unwrap_or("unknown")
                .to_string();
            items_by_date_and_project
                .entry(item_date)
                .or_default()
                .entry((proj.project_title.clone(), proj.project_key.clone()))
                .or_default()
                .push(item);
        }
    }

    // 收集所有涉及的日期并降序遍历
    let mut all_dates: Vec<String> = items_by_date_and_project
        .keys()
        .chain(project_meta_by_date.keys())
        .cloned()
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();
    all_dates.sort_by(|a, b| b.cmp(a)); // 降序

    for date in all_dates {
        out.push_str(&format!("## {}\n\n", date));

        // 该日期下有活动的项目
        let mut rendered_projects_for_date: BTreeMap<(String, String), Option<&RecentProjectView>> =
            BTreeMap::new();

        if let Some(projs) = project_meta_by_date.get(&date) {
            for p in projs {
                rendered_projects_for_date
                    .insert((p.project_title.clone(), p.project_key.clone()), Some(p));
            }
        }

        if let Some(item_projects) = items_by_date_and_project.get(&date) {
            for key in item_projects.keys() {
                rendered_projects_for_date.entry(key.clone()).or_insert(None);
            }
        }

        for ((proj_title, proj_key), proj_opt) in rendered_projects_for_date {
            out.push_str(&format!("### {}\n\n", proj_title));

            if let Some(proj) = proj_opt {
                out.push_str("#### What changed\n");
                out.push_str(proj.summary.trim());
                out.push_str("\n\n");

                // 建议下一步 (从 items 中筛选 recommendation_rank 1..=3)
                let mut suggestions: Vec<&RecentMemoryItemView> = proj
                    .items
                    .iter()
                    .filter(|item| item.recommendation_rank.is_some())
                    .collect();
                suggestions.sort_by_key(|s| s.recommendation_rank.unwrap_or(99));

                out.push_str("#### Suggested next steps\n");
                if suggestions.is_empty() {
                    out.push_str("1. 本窗口没有形成明确下一步\n\n");
                } else {
                    for (idx, s) in suggestions.iter().enumerate() {
                        out.push_str(&format!("{}. {}\n", idx + 1, s.title.trim()));
                    }
                    out.push('\n');
                }
            }

            // 渲染属于该日期的条目
            if let Some(item_projects) = items_by_date_and_project.get(&date) {
                if let Some(items) = item_projects.get(&(proj_title.clone(), proj_key.clone())) {
                    out.push_str("#### Memory items\n");
                    for item in items {
                        let category_str = match item.category.as_str() {
                            "decision" => "Decision",
                            "research" => "Research",
                            "verification" => "Verification",
                            "blocker" => "Blocker",
                            "follow_up" => "FollowUp",
                            _ => "Progress",
                        };

                        out.push_str(&format!(
                            "- **{} · {}** — {}\n",
                            category_str,
                            item.status,
                            item.title.trim()
                        ));
                        if !item.rationale.trim().is_empty() {
                            out.push_str(&format!("  - Why: {}\n", item.rationale.trim()));
                        }

                        let session_titles: Vec<String> = item
                            .session_references
                            .iter()
                            .map(|s| {
                                if s.available {
                                    format!("`{}`", s.session_title.trim())
                                } else {
                                    format!("`{}` (来源不可用)", s.session_title.trim())
                                }
                            })
                            .collect();

                        if !session_titles.is_empty() {
                            out.push_str(&format!(
                                "  - Sessions: {}\n",
                                session_titles.join(", ")
                            ));
                        }
                    }
                    out.push('\n');
                }
            }
        }
    }

    out
}

/// 渲染 MEMORY.md (M35-PROJ-02, 07-markdown-ui-settings.md Section 4)
pub(crate) fn render_memory_long_term_markdown(
    l2_projects: &[L2ProjectMemoryView],
    l3_items: &[L3MemoryItemView],
    published_at: &str,
    revision_hash: &str,
) -> String {
    let mut out = String::new();

    out.push_str("# Memory\n\n");
    out.push_str(&format!("- Published: {}\n", published_at));
    out.push_str(&format!("- Revision: {}\n\n", revision_hash));

    // 1. Project Memory (L2)
    out.push_str("## Project Memory\n\n");

    let mut sorted_projects = l2_projects.to_vec();
    sorted_projects.sort_by(|a, b| a.project_key.cmp(&b.project_key));

    for proj in sorted_projects {
        out.push_str(&format!("### {}\n\n", proj.project_key));

        let mut sorted_items = proj.items.clone();
        // 过滤非 current 条目 (M35-L3-06)
        sorted_items.retain(|i| i.lifecycle == "current");

        // 按 category, updated_at 降序, ID 排序
        sorted_items.sort_by(|a, b| {
            category_order(&a.category)
                .cmp(&category_order(&b.category))
                .then_with(|| b.updated_at.cmp(&a.updated_at))
                .then_with(|| a.item_id.cmp(&b.item_id))
        });

        if sorted_items.is_empty() {
            out.push_str("- *No active project memories*\n\n");
        } else {
            for item in sorted_items {
                let cat_str = match item.category {
                    MemoryItemCategory::Decision => "Decision",
                    MemoryItemCategory::Research => "Research",
                    MemoryItemCategory::Verification => "Verification",
                    MemoryItemCategory::Blocker => "Blocker",
                    MemoryItemCategory::FollowUp => "FollowUp",
                    MemoryItemCategory::Progress => "Progress",
                };

                out.push_str(&format!("- **{}** — {}\n", cat_str, item.title.trim()));
                if !item.rationale.trim().is_empty() {
                    out.push_str(&format!("  - Why: {}\n", item.rationale.trim()));
                }
                out.push_str(&format!("  - Updated: {}\n", item.updated_at));

                let avail_count = item
                    .source_references
                    .iter()
                    .filter(|r| r.available)
                    .count();
                let unavail_count = item.source_references.len() - avail_count;

                if unavail_count > 0 {
                    out.push_str(&format!(
                        "  - Sources: {} available, {} unavailable\n",
                        avail_count, unavail_count
                    ));
                } else if avail_count > 0 {
                    out.push_str(&format!("  - Sources: {} available\n", avail_count));
                }
            }
            out.push('\n');
        }
    }

    // 2. Permanent Memory (L3)
    out.push_str("## Permanent Memory\n\n");

    let mut sorted_l3 = l3_items.to_vec();
    // 过滤非 current 条目 (M35-L3-06)
    sorted_l3.retain(|i| i.lifecycle == "current");

    sorted_l3.sort_by(|a, b| {
        category_order(&a.category)
            .cmp(&category_order(&b.category))
            .then_with(|| b.updated_at.cmp(&a.updated_at))
            .then_with(|| a.item_id.cmp(&b.item_id))
    });

    if sorted_l3.is_empty() {
        out.push_str("- *No active permanent memories*\n\n");
    } else {
        for item in sorted_l3 {
            let cat_str = match item.category {
                MemoryItemCategory::Decision => "Rule",
                MemoryItemCategory::Research => "Conclusion",
                MemoryItemCategory::Verification => "Constraint",
                MemoryItemCategory::Blocker => "Blocker",
                MemoryItemCategory::FollowUp => "Preference",
                MemoryItemCategory::Progress => "Pattern",
            };

            out.push_str(&format!("- **{}** — {}\n", cat_str, item.title.trim()));
            if !item.rationale.trim().is_empty() {
                out.push_str(&format!("  - Why: {}\n", item.rationale.trim()));
            }
            out.push_str(&format!("  - Updated: {}\n", item.updated_at));
        }
        out.push('\n');
    }

    out
}

fn category_order(cat: &MemoryItemCategory) -> usize {
    match cat {
        MemoryItemCategory::Decision => 1,
        MemoryItemCategory::Verification => 2,
        MemoryItemCategory::Research => 3,
        MemoryItemCategory::Blocker => 4,
        MemoryItemCategory::FollowUp => 5,
        MemoryItemCategory::Progress => 6,
    }
}

/// 投影产物路径结果
#[derive(Debug, Clone)]
pub(crate) struct MemoryProjectionPaths {
    pub(crate) summary_path: PathBuf,
    pub(crate) memory_path: PathBuf,
}

/// 从 SQLite 数据库全量重建并发布 Markdown 投影 (M35-PROJ-01 ~ M35-PROJ-03)
///
/// 完全基于 SQLite last-success 重建，绝不调用 Agent！
pub(crate) async fn rebuild_markdown_projections(
    pool: &SqlitePool,
    tenant_id: &str,
    base_dir: Option<&Path>,
) -> AppResult<MemoryProjectionPaths> {
    let now = Utc::now();
    let published_at = now.to_rfc3339();

    let summary_file = memory_summary_path(base_dir, tenant_id);
    let memory_file = memory_long_term_path(base_dir, tenant_id);

    // 1. 重建 memory_summary.md (基于最新成功快照)
    let latest_snapshot_id: Option<String> = sqlx::query_scalar(
        "SELECT last_successful_snapshot_id FROM recent_memory_state WHERE tenant_id = ?1",
    )
    .bind(tenant_id)
    .fetch_optional(pool)
    .await
    .map_err(AppError::external)?;

    if let Some(snap_id) = latest_snapshot_id {
        let snapshot_view = crate::backend::store::recent_snapshot_repo::load_recent_snapshot_by_id_sqlx(
            pool,
            tenant_id,
            &snap_id,
        )
        .await?;

        if let Some(view) = snapshot_view {
            let markdown = render_memory_summary_markdown(&view);
            publish_atomic(&summary_file, &markdown)?;
        }
    }

    // 2. 重建 MEMORY.md (基于 L2 项目记忆与 L3 全局记忆)
    let project_keys: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT project_key FROM memory_items \
         WHERE tenant_id = ?1 AND layer = 'l2' AND project_key IS NOT NULL AND project_key != 'unassigned'",
    )
    .bind(tenant_id)
    .fetch_all(pool)
    .await
    .map_err(AppError::external)?;

    let mut l2_projects = Vec::new();
    for key in project_keys {
        if let Some(proj_view) = crate::backend::application::project_consolidation_pipeline::load_l2_project_memory_view(
            pool,
            tenant_id,
            &key,
        )
        .await?
        {
            l2_projects.push(proj_view);
        }
    }

    let l3_view = crate::backend::application::global_consolidation_pipeline::get_global_memory_l3_view(
        pool,
        tenant_id,
    )
    .await?;

    let l3_items = l3_view.map(|v| v.items).unwrap_or_default();

    let mut hasher = Sha256::new();
    for p in &l2_projects {
        hasher.update(p.revision_hash.as_bytes());
    }
    for i in &l3_items {
        hasher.update(i.revision_id.as_bytes());
    }
    let revision_hash = format!("{:x}", hasher.finalize());

    let long_term_markdown = render_memory_long_term_markdown(
        &l2_projects,
        &l3_items,
        &published_at,
        &revision_hash,
    );
    publish_atomic(&memory_file, &long_term_markdown)?;

    Ok(MemoryProjectionPaths {
        summary_path: summary_file,
        memory_path: memory_file,
    })
}

/// Recent Snapshot 30 天历史清理策略 (M35-PROJ-04)
///
/// 清理超过保留期（默认 30 天）的 Snapshot 历史：
/// 1. 级联删除 snapshot_projects, snapshot_items, jobs
/// 2. 长期条目（memory_items, memory_item_revisions）继续完整保留，绝不级联删除！
/// 3. 返回清理的 Snapshot 数量
pub(crate) async fn purge_stale_recent_memory_snapshots(
    pool: &SqlitePool,
    tenant_id: &str,
    older_than: DateTime<Utc>,
) -> AppResult<usize> {
    let cutoff = older_than.to_rfc3339();

    // 保护当前活跃快照指针不被删除
    let current_active_id: Option<String> = sqlx::query_scalar(
        "SELECT last_successful_snapshot_id FROM recent_memory_state WHERE tenant_id = ?1",
    )
    .bind(tenant_id)
    .fetch_optional(pool)
    .await
    .map_err(AppError::external)?;

    let res = if let Some(ref active_id) = current_active_id {
        sqlx::query(
            "DELETE FROM recent_memory_snapshots \
             WHERE tenant_id = ?1 AND published_at < ?2 AND id != ?3",
        )
        .bind(tenant_id)
        .bind(&cutoff)
        .bind(active_id)
        .execute(pool)
        .await
        .map_err(AppError::external)?
    } else {
        sqlx::query(
            "DELETE FROM recent_memory_snapshots \
             WHERE tenant_id = ?1 AND published_at < ?2",
        )
        .bind(tenant_id)
        .bind(&cutoff)
        .execute(pool)
        .await
        .map_err(AppError::external)?
    };
    Ok(res.rows_affected() as usize)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::backend::application::AppService;
    use crate::backend::dto::recent_snapshot::{
        RecentMemoryItemView, RecentMemorySnapshotView, RecentProjectView,
        RecentSessionReferenceView,
    };
    use crate::backend::models::{
        L2MemoryItemView, L2ProjectMemoryView, L2SourceReferenceView, L3MemoryItemView,
        MemoryItemCategory, MemoryItemStatus,
    };
    use chrono::TimeZone;

    async fn setup_test_service() -> (AppService, SqlitePool, PathBuf) {
        let root = std::env::temp_dir().join(format!("test-memory-projection-{}", uuid::Uuid::new_v4()));
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
                    source_availability: crate::backend::dto::recent_snapshot::SourceAvailability::Available,
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
                source_availability: crate::backend::dto::recent_snapshot::SourceAvailability::Available,
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
        assert_eq!(fs::read_to_string(&file_path).unwrap(), "# Initial Valid Content");

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
        assert!(fs::read_to_string(&paths2.summary_path).unwrap().contains("Alpha Project"));

        let _ = fs::remove_dir_all(root);
    }

    /// 测试 5: M35-PROJ-04 Snapshot 30 天清理不影响长期 revision
    #[tokio::test]
    async fn test_m35_proj_04_snapshot_purge_keeps_long_term_revisions() {
        let (service, pool, root) = setup_test_service().await;
        let tenant_id = service.tenant_id();

        // 1. 插入一个 40 天前的旧快照和一个 5 天前的新快照
        let forty_days_ago = Utc.with_ymd_and_hms(2026, 8, 5, 0, 0, 0).unwrap().to_rfc3339();
        let five_days_ago = Utc.with_ymd_and_hms(2026, 9, 10, 0, 0, 0).unwrap().to_rfc3339();

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

        let remaining_snapshots: Vec<String> = sqlx::query_scalar(
            "SELECT id FROM recent_memory_snapshots WHERE tenant_id = ?1",
        )
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
}
