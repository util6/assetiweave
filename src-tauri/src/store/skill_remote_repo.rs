use crate::types::{AppResult, SkillRemoteSource};
use rusqlite::{params, Connection, OptionalExtension, Row};

use super::{codec::db_error, sql};

pub(crate) fn list_skill_remote_sources(conn: &Connection) -> AppResult<Vec<SkillRemoteSource>> {
    let mut stmt = conn
        .prepare(sql::LIST_SKILL_REMOTE_SOURCES)
        .map_err(db_error)?;
    let rows = stmt
        .query_map([], map_skill_remote_source)
        .map_err(db_error)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(db_error)
}

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

pub(crate) fn delete_orphan_skill_remote_sources(conn: &Connection) -> AppResult<()> {
    conn.execute(sql::DELETE_ORPHAN_SKILL_REMOTE_SOURCES, [])
        .map_err(db_error)?;
    Ok(())
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::sql;

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
}
