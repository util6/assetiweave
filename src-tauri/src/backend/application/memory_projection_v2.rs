use crate::backend::dto::recent_snapshot::{
    RecentMemoryItemView, RecentMemorySnapshotView, RecentProjectView,
    RecentSnapshotPublicationKind,
};
use crate::backend::models::{L2ProjectMemoryView, L3MemoryItemView, MemoryItemCategory};
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
    let mut items_by_date_and_project: BTreeMap<
        String,
        BTreeMap<(String, String), Vec<&RecentMemoryItemView>>,
    > = BTreeMap::new();

    let mut project_meta_by_date: BTreeMap<String, Vec<&RecentProjectView>> = BTreeMap::new();

    for proj in &snapshot.projects {
        let proj_date = proj
            .latest_activity_at
            .split('T')
            .next()
            .unwrap_or("unknown")
            .to_string();
        project_meta_by_date
            .entry(proj_date)
            .or_default()
            .push(proj);

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
                rendered_projects_for_date
                    .entry(key.clone())
                    .or_insert(None);
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
                            out.push_str(&format!("  - Sessions: {}\n", session_titles.join(", ")));
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
        let snapshot_view =
            crate::backend::store::recent_snapshot_repo::load_recent_snapshot_by_id_sqlx(
                pool, tenant_id, &snap_id,
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

    let l3_view =
        crate::backend::application::global_consolidation_pipeline::get_global_memory_l3_view(
            pool, tenant_id,
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

    let long_term_markdown =
        render_memory_long_term_markdown(&l2_projects, &l3_items, &published_at, &revision_hash);
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
#[path = "memory_projection_v2_tests.rs"]
pub(crate) mod tests;
