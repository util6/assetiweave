use crate::backend::{dto::SkillRemoteSource, runtime::AppResult};
use sqlx::{sqlite::SqliteRow, Row as SqlxRow, SqlitePool};

use super::sql;

pub(crate) async fn list_skill_remote_sources_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
) -> AppResult<Vec<SkillRemoteSource>> {
    let rows = sqlx::query(sql::LIST_SKILL_REMOTE_SOURCES)
        .bind(tenant_id)
        .fetch_all(pool)
        .await?;
    rows.iter().map(map_sqlx_skill_remote_source).collect()
}

pub(crate) async fn load_skill_remote_source_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    asset_id: &str,
) -> AppResult<Option<SkillRemoteSource>> {
    sqlx::query(sql::GET_SKILL_REMOTE_SOURCE)
        .bind(tenant_id)
        .bind(asset_id)
        .fetch_optional(pool)
        .await?
        .as_ref()
        .map(map_sqlx_skill_remote_source)
        .transpose()
}

pub(crate) async fn upsert_skill_remote_source_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    source: &SkillRemoteSource,
) -> AppResult<()> {
    sqlx::query(sql::UPSERT_SKILL_REMOTE_SOURCE)
        .bind(tenant_id)
        .bind(&source.asset_id)
        .bind(&source.provider)
        .bind(&source.source_url)
        .bind(&source.repo_url)
        .bind(&source.branch)
        .bind(&source.path)
        .bind(&source.acquired_at)
        .bind(&source.acquired_tree_sha)
        .bind(&source.local_content_hash)
        .bind(&source.last_checked_at)
        .bind(&source.latest_tree_sha)
        .bind(&source.status)
        .bind(&source.message)
        .execute(pool)
        .await?;
    Ok(())
}

pub(crate) async fn update_skill_remote_check_result_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    source: &SkillRemoteSource,
) -> AppResult<()> {
    sqlx::query(sql::UPDATE_SKILL_REMOTE_CHECK)
        .bind(tenant_id)
        .bind(&source.asset_id)
        .bind(&source.last_checked_at)
        .bind(&source.latest_tree_sha)
        .bind(&source.status)
        .bind(&source.message)
        .execute(pool)
        .await?;
    Ok(())
}

pub(crate) async fn delete_orphan_skill_remote_sources_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
) -> AppResult<()> {
    sqlx::query(sql::DELETE_ORPHAN_SKILL_REMOTE_SOURCES)
        .bind(tenant_id)
        .execute(pool)
        .await?;
    Ok(())
}

fn map_sqlx_skill_remote_source(row: &SqliteRow) -> AppResult<SkillRemoteSource> {
    Ok(SkillRemoteSource {
        asset_id: row.try_get(0)?,
        provider: row.try_get(1)?,
        source_url: row.try_get(2)?,
        repo_url: row.try_get(3)?,
        branch: row.try_get(4)?,
        path: row.try_get(5)?,
        acquired_at: row.try_get(6)?,
        acquired_tree_sha: row.try_get(7)?,
        local_content_hash: row.try_get(8)?,
        last_checked_at: row.try_get(9)?,
        latest_tree_sha: row.try_get(10)?,
        status: row.try_get(11)?,
        message: row.try_get(12)?,
    })
}

#[cfg(test)]
#[path = "skill_remote_repo_tests.rs"]
mod tests;
