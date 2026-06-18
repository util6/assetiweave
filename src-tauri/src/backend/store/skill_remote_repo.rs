use crate::backend::dto::{AppResult, SkillRemoteSource};
#[cfg(test)]
use rusqlite::{params, Connection, OptionalExtension, Row};
use sqlx::{sqlite::SqliteRow, Row as SqlxRow, SqlitePool};

#[cfg(test)]
use super::codec::db_error;
use super::sql;

#[cfg(test)]
pub(crate) fn list_skill_remote_sources(conn: &Connection) -> AppResult<Vec<SkillRemoteSource>> {
    let mut stmt = conn
        .prepare(sql::LIST_SKILL_REMOTE_SOURCES)
        .map_err(db_error)?;
    let rows = stmt
        .query_map([], map_skill_remote_source)
        .map_err(db_error)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(db_error)
}

pub(crate) async fn list_skill_remote_sources_sqlx(
    pool: &SqlitePool,
) -> AppResult<Vec<SkillRemoteSource>> {
    let rows = sqlx::query(sql::LIST_SKILL_REMOTE_SOURCES)
        .fetch_all(pool)
        .await
        .map_err(|error| error.to_string())?;
    rows.iter().map(map_sqlx_skill_remote_source).collect()
}

#[cfg(test)]
pub(crate) fn load_skill_remote_source(
    conn: &Connection,
    asset_id: &str,
) -> AppResult<Option<SkillRemoteSource>> {
    conn.query_row(
        sql::GET_SKILL_REMOTE_SOURCE,
        params![asset_id],
        map_skill_remote_source,
    )
    .optional()
    .map_err(db_error)
}

pub(crate) async fn load_skill_remote_source_sqlx(
    pool: &SqlitePool,
    asset_id: &str,
) -> AppResult<Option<SkillRemoteSource>> {
    sqlx::query(sql::GET_SKILL_REMOTE_SOURCE)
        .bind(asset_id)
        .fetch_optional(pool)
        .await
        .map_err(|error| error.to_string())?
        .as_ref()
        .map(map_sqlx_skill_remote_source)
        .transpose()
}

#[cfg(test)]
pub(crate) fn upsert_skill_remote_source(
    conn: &Connection,
    source: &SkillRemoteSource,
) -> AppResult<()> {
    conn.execute(
        sql::UPSERT_SKILL_REMOTE_SOURCE,
        params![
            source.asset_id,
            source.provider,
            source.source_url,
            source.repo_url,
            source.branch,
            source.path,
            source.acquired_at,
            source.acquired_tree_sha,
            source.local_content_hash,
            source.last_checked_at,
            source.latest_tree_sha,
            source.status,
            source.message,
        ],
    )
    .map_err(db_error)?;
    Ok(())
}

pub(crate) async fn upsert_skill_remote_source_sqlx(
    pool: &SqlitePool,
    source: &SkillRemoteSource,
) -> AppResult<()> {
    sqlx::query(sql::UPSERT_SKILL_REMOTE_SOURCE)
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
        .map_err(|error| error.to_string())?;
    Ok(())
}

#[cfg(test)]
pub(crate) fn update_skill_remote_check_result(
    conn: &Connection,
    source: &SkillRemoteSource,
) -> AppResult<()> {
    conn.execute(
        sql::UPDATE_SKILL_REMOTE_CHECK,
        params![
            source.asset_id,
            source.last_checked_at,
            source.latest_tree_sha,
            source.status,
            source.message,
        ],
    )
    .map_err(db_error)?;
    Ok(())
}

pub(crate) async fn update_skill_remote_check_result_sqlx(
    pool: &SqlitePool,
    source: &SkillRemoteSource,
) -> AppResult<()> {
    sqlx::query(sql::UPDATE_SKILL_REMOTE_CHECK)
        .bind(&source.asset_id)
        .bind(&source.last_checked_at)
        .bind(&source.latest_tree_sha)
        .bind(&source.status)
        .bind(&source.message)
        .execute(pool)
        .await
        .map_err(|error| error.to_string())?;
    Ok(())
}

pub(crate) async fn delete_orphan_skill_remote_sources_sqlx(pool: &SqlitePool) -> AppResult<()> {
    sqlx::query(sql::DELETE_ORPHAN_SKILL_REMOTE_SOURCES)
        .execute(pool)
        .await
        .map_err(|error| error.to_string())?;
    Ok(())
}

#[cfg(test)]
fn map_skill_remote_source(row: &Row<'_>) -> rusqlite::Result<SkillRemoteSource> {
    Ok(SkillRemoteSource {
        asset_id: row.get(0)?,
        provider: row.get(1)?,
        source_url: row.get(2)?,
        repo_url: row.get(3)?,
        branch: row.get(4)?,
        path: row.get(5)?,
        acquired_at: row.get(6)?,
        acquired_tree_sha: row.get(7)?,
        local_content_hash: row.get(8)?,
        last_checked_at: row.get(9)?,
        latest_tree_sha: row.get(10)?,
        status: row.get(11)?,
        message: row.get(12)?,
    })
}

fn map_sqlx_skill_remote_source(row: &SqliteRow) -> AppResult<SkillRemoteSource> {
    Ok(SkillRemoteSource {
        asset_id: row.try_get(0).map_err(|error| error.to_string())?,
        provider: row.try_get(1).map_err(|error| error.to_string())?,
        source_url: row.try_get(2).map_err(|error| error.to_string())?,
        repo_url: row.try_get(3).map_err(|error| error.to_string())?,
        branch: row.try_get(4).map_err(|error| error.to_string())?,
        path: row.try_get(5).map_err(|error| error.to_string())?,
        acquired_at: row.try_get(6).map_err(|error| error.to_string())?,
        acquired_tree_sha: row.try_get(7).map_err(|error| error.to_string())?,
        local_content_hash: row.try_get(8).map_err(|error| error.to_string())?,
        last_checked_at: row.try_get(9).map_err(|error| error.to_string())?,
        latest_tree_sha: row.try_get(10).map_err(|error| error.to_string())?,
        status: row.try_get(11).map_err(|error| error.to_string())?,
        message: row.try_get(12).map_err(|error| error.to_string())?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::models::{Asset, AssetFormat, AssetKind};
    use crate::backend::store::sql;

    #[test]
    fn upsert_and_update_skill_remote_source() {
        let conn = Connection::open_in_memory().expect("open in-memory db");
        conn.execute_batch(sql::INIT_SCHEMA).expect("init schema");
        let mut source = SkillRemoteSource {
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
        };

        upsert_skill_remote_source(&conn, &source).expect("upsert remote source");
        source.last_checked_at = Some("2026-01-02T00:00:00Z".to_string());
        source.latest_tree_sha = Some("new-tree".to_string());
        source.status = "changed".to_string();
        source.message = Some("Remote Skill changed since import".to_string());
        update_skill_remote_check_result(&conn, &source).expect("update remote check");

        let loaded = load_skill_remote_source(&conn, "asset-a")
            .expect("load remote source")
            .expect("remote source exists");
        assert_eq!(loaded.status, "changed");
        assert_eq!(loaded.latest_tree_sha.as_deref(), Some("new-tree"));
        assert_eq!(
            list_skill_remote_sources(&conn)
                .expect("list remote sources")
                .len(),
            1
        );
    }

    #[test]
    fn sqlx_upserts_and_updates_skill_remote_source() {
        let db_path = std::env::temp_dir().join(format!(
            "assetiweave-skill-remote-sqlx-{}.sqlite",
            uuid::Uuid::new_v4()
        ));
        let database = crate::backend::store::Database::open(&db_path).expect("open database");
        let mut source = test_remote_source();

        database
            .block_on(async {
                upsert_skill_remote_source_sqlx(database.pool(), &source).await?;
                source.last_checked_at = Some("2026-01-02T00:00:00Z".to_string());
                source.latest_tree_sha = Some("new-tree".to_string());
                source.status = "changed".to_string();
                source.message = Some("Remote Skill changed since import".to_string());
                update_skill_remote_check_result_sqlx(database.pool(), &source).await?;
                let loaded = load_skill_remote_source_sqlx(database.pool(), "asset-a").await?;
                let listed = list_skill_remote_sources_sqlx(database.pool()).await?;
                AppResult::Ok((loaded, listed))
            })
            .map(|(loaded, listed)| {
                assert_eq!(loaded.expect("loaded remote").status, "changed");
                assert_eq!(listed.len(), 1);
            })
            .expect("query SQLx skill remote repo");
        drop(database);
        let _ = std::fs::remove_file(&db_path);
        let _ = std::fs::remove_file(db_path.with_extension("sqlite-wal"));
        let _ = std::fs::remove_file(db_path.with_extension("sqlite-shm"));
    }

    #[test]
    fn sqlx_deletes_orphan_skill_remote_sources() {
        let db_path = std::env::temp_dir().join(format!(
            "assetiweave-skill-remote-orphan-sqlx-{}.sqlite",
            uuid::Uuid::new_v4()
        ));
        let database = crate::backend::store::Database::open(&db_path).expect("open database");
        let asset = test_asset();
        let retained = test_remote_source();
        let mut orphan = test_remote_source();
        orphan.asset_id = "missing-asset".to_string();

        let listed = database
            .block_on(async {
                crate::backend::store::replace_source_assets_sqlx(
                    database.pool(),
                    &asset.source_id,
                    std::slice::from_ref(&asset),
                )
                .await?;
                upsert_skill_remote_source_sqlx(database.pool(), &retained).await?;
                upsert_skill_remote_source_sqlx(database.pool(), &orphan).await?;
                delete_orphan_skill_remote_sources_sqlx(database.pool()).await?;
                list_skill_remote_sources_sqlx(database.pool()).await
            })
            .expect("delete SQLx orphan remote sources");

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
