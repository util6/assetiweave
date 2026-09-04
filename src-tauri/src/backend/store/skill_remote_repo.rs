use crate::backend::{
    dto::SkillRemoteSource,
    runtime::{AppError, AppResult},
};
use sqlx::{sqlite::SqliteRow, Row as SqlxRow, SqlitePool};

use super::sql;

pub(crate) async fn list_skill_remote_sources_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
) -> AppResult<Vec<SkillRemoteSource>> {
    let rows = sqlx::query(sql::LIST_SKILL_REMOTE_SOURCES)
        .bind(tenant_id)
        .fetch_all(pool)
        .await
        .map_err(AppError::external)?;
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
        .await
        .map_err(AppError::external)?
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
        .await
        .map_err(AppError::external)?;
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
        .await
        .map_err(AppError::external)?;
    Ok(())
}

pub(crate) async fn delete_orphan_skill_remote_sources_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
) -> AppResult<()> {
    sqlx::query(sql::DELETE_ORPHAN_SKILL_REMOTE_SOURCES)
        .bind(tenant_id)
        .execute(pool)
        .await
        .map_err(AppError::external)?;
    Ok(())
}

fn map_sqlx_skill_remote_source(row: &SqliteRow) -> AppResult<SkillRemoteSource> {
    Ok(SkillRemoteSource {
        asset_id: row.try_get(0).map_err(AppError::external)?,
        provider: row.try_get(1).map_err(AppError::external)?,
        source_url: row.try_get(2).map_err(AppError::external)?,
        repo_url: row.try_get(3).map_err(AppError::external)?,
        branch: row.try_get(4).map_err(AppError::external)?,
        path: row.try_get(5).map_err(AppError::external)?,
        acquired_at: row.try_get(6).map_err(AppError::external)?,
        acquired_tree_sha: row.try_get(7).map_err(AppError::external)?,
        local_content_hash: row.try_get(8).map_err(AppError::external)?,
        last_checked_at: row.try_get(9).map_err(AppError::external)?,
        latest_tree_sha: row.try_get(10).map_err(AppError::external)?,
        status: row.try_get(11).map_err(AppError::external)?,
        message: row.try_get(12).map_err(AppError::external)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::models::{Asset, AssetFormat, AssetKind};

    #[tokio::test]
    async fn sqlx_skill_remote_source_round_trips_and_updates_check_result() {
        let db_path = std::env::temp_dir().join(format!(
            "assetiweave-skill-remote-sqlx-{}.sqlite",
            uuid::Uuid::new_v4()
        ));
        let database = crate::backend::store::Database::open_async(&db_path)
            .await
            .expect("open database");
        let mut source = test_remote_source();

        upsert_skill_remote_source_sqlx(database.pool(), "default", &source)
            .await
            .expect("upsert skill remote source");
        source.last_checked_at = Some("2026-01-02T00:00:00Z".to_string());
        source.latest_tree_sha = Some("new-tree".to_string());
        source.status = "changed".to_string();
        source.message = Some("Remote Skill changed since import".to_string());
        update_skill_remote_check_result_sqlx(database.pool(), "default", &source)
            .await
            .expect("update check result");
        let loaded = load_skill_remote_source_sqlx(database.pool(), "default", "asset-a")
            .await
            .expect("load remote source");
        let listed = list_skill_remote_sources_sqlx(database.pool(), "default")
            .await
            .expect("list remote sources");

        assert_eq!(loaded.expect("loaded remote").status, "changed");
        assert_eq!(listed.len(), 1);

        drop(database);
        let _ = std::fs::remove_file(&db_path);
        let _ = std::fs::remove_file(db_path.with_extension("sqlite-wal"));
        let _ = std::fs::remove_file(db_path.with_extension("sqlite-shm"));
    }

    #[tokio::test]
    async fn sqlx_deletes_orphan_skill_remote_sources() {
        let db_path = std::env::temp_dir().join(format!(
            "assetiweave-skill-remote-orphan-sqlx-{}.sqlite",
            uuid::Uuid::new_v4()
        ));
        let database = crate::backend::store::Database::open_async(&db_path)
            .await
            .expect("open database");
        let asset = test_asset();
        let retained = test_remote_source();
        let mut orphan = test_remote_source();
        orphan.asset_id = "missing-asset".to_string();

        crate::backend::store::replace_source_assets_sqlx(
            database.pool(),
            "default",
            &asset.source_id,
            std::slice::from_ref(&asset),
        )
        .await
        .expect("replace source assets");
        upsert_skill_remote_source_sqlx(database.pool(), "default", &retained)
            .await
            .expect("upsert retained");
        upsert_skill_remote_source_sqlx(database.pool(), "default", &orphan)
            .await
            .expect("upsert orphan");
        delete_orphan_skill_remote_sources_sqlx(database.pool(), "default")
            .await
            .expect("delete orphan remote sources");
        let listed = list_skill_remote_sources_sqlx(database.pool(), "default")
            .await
            .expect("list remaining remote sources");

        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].asset_id, retained.asset_id);
        drop(database);
        let _ = std::fs::remove_file(&db_path);
        let _ = std::fs::remove_file(db_path.with_extension("sqlite-wal"));
        let _ = std::fs::remove_file(db_path.with_extension("sqlite-shm"));
    }

    fn test_asset() -> Asset {
        Asset {
            id: "asset-a".to_string(),
            source_id: "source-a".to_string(),
            name: "Skill A".to_string(),
            kind: AssetKind::Skill,
            detector_id: "legacy.classifier".to_string(),
            detector_version: 1,
            format: AssetFormat::Directory,
            relative_path: "skill-a".to_string(),
            absolute_path: "/tmp/source-a/skill-a".to_string(),
            entry_file: Some("/tmp/source-a/skill-a/SKILL.md".to_string()),
            description: None,
            content_hash: Some("hash-a".to_string()),
            discovered_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        }
    }

    fn test_remote_source() -> SkillRemoteSource {
        SkillRemoteSource {
            asset_id: "asset-a".to_string(),
            provider: "github".to_string(),
            source_url: "https://github.com/example/repo/tree/main/skill".to_string(),
            repo_url: "https://github.com/example/repo.git".to_string(),
            branch: "main".to_string(),
            path: Some("skill".to_string()),
            acquired_at: "2026-01-01T00:00:00Z".to_string(),
            acquired_tree_sha: Some("old-tree".to_string()),
            local_content_hash: Some("local".to_string()),
            last_checked_at: None,
            latest_tree_sha: None,
            status: "unknown".to_string(),
            message: None,
        }
    }
}
