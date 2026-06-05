use crate::{defaults, path_utils::ensure_app_library_dirs, types::AppResult};
use rusqlite::{Connection, OptionalExtension};
use std::path::Path;

use super::{
    codec::db_error,
    menu_repo::{ensure_navigation_model_items, seed_navigation_model},
    profile_repo::{load_profiles, upsert_profile},
    seed_builtin_conversation_adapters,
    shortcut_repo::seed_app_shortcuts,
    source_repo::{load_sources, upsert_source},
    sql,
};

pub(crate) fn open_initialized(db_path: &Path) -> AppResult<Connection> {
    let conn = Connection::open(db_path).map_err(db_error)?;
    init_schema(&conn)?;
    seed_defaults(&conn)?;
    Ok(conn)
}

fn init_schema(conn: &Connection) -> AppResult<()> {
    conn.execute_batch(sql::INIT_SCHEMA).map_err(db_error)?;
    migrate_schema(conn)?;
    ensure_app_library_dirs()?;
    Ok(())
}

fn migrate_schema(conn: &Connection) -> AppResult<()> {
    ensure_column(
        conn,
        "sources",
        "scanner_kind",
        sql::ADD_SOURCE_SCANNER_KIND,
    )?;
    ensure_column(conn, "sources", "source_origin", sql::ADD_SOURCE_ORIGIN)?;
    ensure_column(conn, "sources", "repo_root", sql::ADD_SOURCE_REPO_ROOT)?;
    ensure_column(conn, "sources", "scan_root", sql::ADD_SOURCE_SCAN_ROOT)?;
    ensure_column(
        conn,
        "sources",
        "origin_app_kind",
        sql::ADD_SOURCE_ORIGIN_APP_KIND,
    )?;
    ensure_column(
        conn,
        "rail_menu_items",
        "label_zh",
        sql::ADD_RAIL_MENU_LABEL_ZH,
    )?;
    ensure_column(
        conn,
        "rail_menu_items",
        "label_en",
        sql::ADD_RAIL_MENU_LABEL_EN,
    )?;
    ensure_column(
        conn,
        "header_tab_items",
        "label_zh",
        sql::ADD_HEADER_TAB_LABEL_ZH,
    )?;
    ensure_column(
        conn,
        "header_tab_items",
        "label_en",
        sql::ADD_HEADER_TAB_LABEL_EN,
    )?;
    ensure_column(conn, "sub_nav_items", "label_zh", sql::ADD_SUB_NAV_LABEL_ZH)?;
    ensure_column(conn, "sub_nav_items", "label_en", sql::ADD_SUB_NAV_LABEL_EN)?;
    ensure_column(
        conn,
        "app_shortcut_items",
        "icon_svg",
        sql::ADD_APP_SHORTCUT_ICON_SVG,
    )?;
    conn.execute_batch(sql::MIGRATE_DEPLOYMENT_STATE_STRATEGY_NAMES)
        .map_err(db_error)?;
    conn.execute_batch(sql::MIGRATE_ASSET_MOUNT_STRATEGY_NAMES)
        .map_err(db_error)?;
    Ok(())
}

fn ensure_column(
    conn: &Connection,
    table: &str,
    column: &str,
    alter_statement: &str,
) -> AppResult<()> {
    if table_columns(conn, table)?
        .iter()
        .any(|name| name == column)
    {
        return Ok(());
    }
    conn.execute(alter_statement, []).map_err(db_error)?;
    Ok(())
}

fn table_columns(conn: &Connection, table: &str) -> AppResult<Vec<String>> {
    let mut stmt = conn
        .prepare(&format!("PRAGMA table_info({table})"))
        .map_err(db_error)?;
    let rows = stmt
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(db_error)?;
    rows.collect::<Result<Vec<_>, _>>().map_err(db_error)
}

fn seed_defaults(conn: &Connection) -> AppResult<()> {
    if count_rows(conn, "sources")? == 0 {
        for source in defaults::default_sources() {
            upsert_source(conn, &source)?;
        }
    }
    ensure_library_source(conn)?;
    normalize_existing_sources(conn)?;

    if count_rows(conn, "profiles")? == 0 {
        for profile in defaults::default_profiles() {
            upsert_profile(conn, &profile)?;
        }
    }
    normalize_default_profiles(conn)?;

    seed_builtin_conversation_adapters(conn)?;

    if count_rows(conn, "navigation_state")? == 0 {
        seed_navigation_model(conn, &defaults::default_navigation_model())?;
    } else {
        ensure_navigation_model_items(conn, &defaults::default_navigation_model())?;
    }

    if count_rows(conn, "app_shortcut_items")? == 0 {
        seed_app_shortcuts(conn, &defaults::default_app_shortcuts())?;
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
        if let Some(source) = defaults::default_sources()
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
    let defaults = defaults::default_profiles();
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
