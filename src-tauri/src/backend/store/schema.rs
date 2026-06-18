use crate::backend::{dto::AppResult, path_utils::ensure_app_library_dirs};
use rusqlite::{Connection, OptionalExtension};
use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
    time::Duration,
};

use super::{
    codec::db_error,
    database::migrate_database,
    menu_repo::{ensure_navigation_model_items, seed_navigation_model},
    profile_repo::{load_profiles, upsert_profile},
    seed_builtin_conversation_adapters,
    shortcut_repo::seed_app_shortcuts,
    source_repo::{load_sources, upsert_source},
    sql,
};

static INITIALIZED_DB_PATHS: OnceLock<Mutex<BTreeSet<PathBuf>>> = OnceLock::new();

pub(crate) fn open_initialized(db_path: &Path) -> AppResult<Connection> {
    let initialized_paths = INITIALIZED_DB_PATHS.get_or_init(|| Mutex::new(BTreeSet::new()));
    let mut initialized_paths = initialized_paths
        .lock()
        .map_err(|error| error.to_string())?;
    if !initialized_paths.contains(db_path) {
        migrate_database(db_path)?;
    }
    let conn = Connection::open(db_path).map_err(db_error)?;
    configure_connection(&conn)?;
    if !initialized_paths.contains(db_path) {
        ensure_app_library_dirs()?;
        seed_defaults(&conn)?;
        initialized_paths.insert(db_path.to_path_buf());
    }
    Ok(conn)
}

fn configure_connection(conn: &Connection) -> AppResult<()> {
    conn.busy_timeout(Duration::from_secs(10))
        .map_err(db_error)?;
    let journal_mode: String = conn
        .pragma_query_value(None, "journal_mode", |row| row.get(0))
        .map_err(db_error)?;
    if !journal_mode.eq_ignore_ascii_case("wal") {
        conn.pragma_update(None, "journal_mode", "WAL")
            .map_err(db_error)?;
    }
    conn.pragma_update(None, "synchronous", "NORMAL")
        .map_err(db_error)?;
    conn.pragma_update(None, "foreign_keys", "ON")
        .map_err(db_error)?;
    Ok(())
}

fn seed_defaults(conn: &Connection) -> AppResult<()> {
    if count_rows(conn, "sources")? == 0 {
        for source in crate::backend::defaults::default_sources() {
            upsert_source(conn, &source)?;
        }
    }
    ensure_library_source(conn)?;
    normalize_existing_sources(conn)?;

    if count_rows(conn, "profiles")? == 0 {
        for profile in crate::backend::defaults::default_profiles() {
            upsert_profile(conn, &profile)?;
        }
    }
    normalize_default_profiles(conn)?;

    seed_builtin_conversation_adapters(conn)?;

    if count_rows(conn, "navigation_state")? == 0 {
        seed_navigation_model(conn, &crate::backend::defaults::default_navigation_model())?;
    } else {
        ensure_navigation_model_items(conn, &crate::backend::defaults::default_navigation_model())?;
    }

    if count_rows(conn, "app_shortcut_items")? == 0 {
        seed_app_shortcuts(conn, &crate::backend::defaults::default_app_shortcuts())?;
    }

    Ok(())
}

fn ensure_library_source(conn: &Connection) -> AppResult<()> {
    let exists: bool = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sources WHERE id = 'assetiweave-library-skills')",
            [],
            |row| row.get::<_, i64>(0),
        )
        .map_err(db_error)?
        == 1;
    if !exists {
        if let Some(source) = crate::backend::defaults::default_sources()
            .into_iter()
            .find(|source| source.id == "assetiweave-library-skills")
        {
            upsert_source(conn, &source)?;
        }
    }
    Ok(())
}

fn normalize_existing_sources(conn: &Connection) -> AppResult<()> {
    for source in load_sources(conn)? {
        upsert_source(conn, &source)?;
    }
    Ok(())
}

fn normalize_default_profiles(conn: &Connection) -> AppResult<()> {
    let defaults = crate::backend::defaults::default_profiles();
    for mut profile in load_profiles(conn)? {
        let Some(default_profile) = defaults.iter().find(|candidate| candidate.id == profile.id)
        else {
            continue;
        };
        if legacy_profile_target_paths(&profile.id).contains(&profile.target_paths) {
            profile.target_paths = default_profile.target_paths.clone();
            upsert_profile(conn, &profile)?;
        }
    }
    Ok(())
}

fn legacy_profile_target_paths(profile_id: &str) -> Vec<Vec<String>> {
    let legacy_path = match profile_id {
        "codex" => "~/.codex/assetiweave",
        "claude" => "~/.claude/assetiweave",
        "cursor" => "~/Library/Application Support/Cursor/assetiweave",
        "opencode" => "~/.opencode/assetiweave",
        "gemini" => "~/.gemini/assetiweave",
        "antigravity" => "~/.antigravity/assetiweave",
        "openclaw" => "~/.openclaw/assetiweave",
        "custom" => "~/assetiweave-target",
        _ => return Vec::new(),
    };
    let mut paths = vec![vec![legacy_path.to_string()]];
    if profile_id == "opencode" {
        paths.push(vec!["~/.opencode/skills".to_string()]);
    }
    paths
}

pub(crate) fn latest_scan_status(conn: &Connection) -> AppResult<String> {
    let status: Option<String> = conn
        .query_row(sql::LATEST_SCAN_STATUS, [], |row| row.get(0))
        .optional()
        .map_err(db_error)?
        .flatten();
    Ok(status.unwrap_or_else(|| "等待首次扫描".to_string()))
}

pub(crate) fn count_rows(conn: &Connection, table: &str) -> AppResult<usize> {
    let statement = match table {
        "sources" => sql::COUNT_SOURCES,
        "assets" => sql::COUNT_ASSETS,
        "profiles" => sql::COUNT_PROFILES,
        "navigation_state" => sql::COUNT_NAVIGATION_STATE,
        "app_shortcut_items" => sql::COUNT_APP_SHORTCUTS,
        other => return Err(format!("unsupported count table: {other}")),
    };
    let count: i64 = conn
        .query_row(statement, [], |row| row.get(0))
        .map_err(db_error)?;
    Ok(count as usize)
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn reopening_initialized_database_does_not_reseed_defaults() {
        let db_path = std::env::temp_dir().join(format!(
            "assetiweave-schema-reopen-{}.sqlite",
            Uuid::new_v4()
        ));
        let conn = open_initialized(&db_path).unwrap();
        conn.execute(
            "UPDATE conversation_adapters SET name = 'preserved' WHERE id = 'codex'",
            [],
        )
        .unwrap();
        drop(conn);

        let reopened = open_initialized(&db_path).unwrap();
        let name: String = reopened
            .query_row(
                "SELECT name FROM conversation_adapters WHERE id = 'codex'",
                [],
                |row| row.get(0),
            )
            .unwrap();

        assert_eq!(name, "preserved");
        drop(reopened);
        let _ = std::fs::remove_file(&db_path);
        let _ = std::fs::remove_file(db_path.with_extension("sqlite-wal"));
        let _ = std::fs::remove_file(db_path.with_extension("sqlite-shm"));
    }
}
