use super::memory_projection_markdown::*;
use crate::backend::application::{AppError, AppResult};
use crate::backend::domain::memory::{
    RecentMemoryItemView, RecentMemorySnapshotView, RecentProjectView,
    RecentSnapshotPublicationKind,
};
use crate::backend::domain::{L2ProjectMemoryView, L3MemoryItemView, MemoryItemCategory};
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
            crate::backend::application::memory::recent::load_recent_snapshot_view_by_id(
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
        if let Some(proj_view) = crate::backend::application::memory::project_consolidation_pipeline::load_l2_project_memory_view(
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
        crate::backend::application::memory::global_consolidation_pipeline::get_global_memory_l3_view(
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
#[path = "memory_projection_tests.rs"]
pub(crate) mod tests;
