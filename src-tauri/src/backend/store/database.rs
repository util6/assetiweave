use crate::backend::{dto::AppResult, path_utils::ensure_app_library_dirs};
use sqlx::{
    migrate::Migrator,
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous},
    AssertSqlSafe, Row, SqlitePool,
};
use std::{
    collections::BTreeSet,
    future::Future,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
    time::Duration,
};
use tokio::runtime::Runtime;

static MIGRATOR: Migrator = sqlx::migrate!("./migrations");
static INITIALIZED_DB_PATHS: OnceLock<Mutex<BTreeSet<PathBuf>>> = OnceLock::new();

pub(crate) struct Database {
    pool: SqlitePool,
    runtime: Runtime,
}

impl Database {
    #[cfg(test)]
    pub(crate) fn open(db_path: &Path) -> AppResult<Self> {
        let runtime = build_runtime()?;
        let pool = runtime.block_on(open_migrated_pool(db_path))?;
        Ok(Self { pool, runtime })
    }

    pub(crate) fn open_initialized(db_path: &Path) -> AppResult<Self> {
        let runtime = build_runtime()?;
        let pool = runtime.block_on(open_migrated_pool(db_path))?;
        let initialized_paths = INITIALIZED_DB_PATHS.get_or_init(|| Mutex::new(BTreeSet::new()));
        let mut initialized_paths = initialized_paths
            .lock()
            .map_err(|error| error.to_string())?;
        if !initialized_paths.contains(db_path) {
            ensure_app_library_dirs()?;
            runtime.block_on(seed_defaults_sqlx(&pool))?;
            initialized_paths.insert(db_path.to_path_buf());
        }
        Ok(Self { pool, runtime })
    }

    pub(crate) fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    pub(crate) fn block_on<F: Future>(&self, future: F) -> F::Output {
        self.runtime.block_on(future)
    }
}

pub(crate) async fn latest_scan_status(pool: &SqlitePool, tenant_id: &str) -> AppResult<String> {
    let status: Option<String> = sqlx::query_scalar(
        "SELECT last_scan_status FROM sources WHERE tenant_id = ?1 ORDER BY last_scanned_at DESC NULLS LAST LIMIT 1",
    )
    .bind(tenant_id)
    .fetch_optional(pool)
    .await
    .map_err(|error| error.to_string())?
    .flatten();
    Ok(status.unwrap_or_else(|| "等待首次扫描".to_string()))
}

pub(crate) async fn count_rows(
    pool: &SqlitePool,
    tenant_id: &str,
    table: &str,
) -> AppResult<usize> {
    let count: i64 = match table {
        "sources" => sqlx::query_scalar("SELECT COUNT(*) FROM sources WHERE tenant_id = ?1")
            .bind(tenant_id)
            .fetch_one(pool)
            .await
            .map_err(|error| error.to_string())?,
        "assets" => sqlx::query_scalar("SELECT COUNT(*) FROM assets WHERE tenant_id = ?1")
            .bind(tenant_id)
            .fetch_one(pool)
            .await
            .map_err(|error| error.to_string())?,
        "profiles" => sqlx::query_scalar("SELECT COUNT(*) FROM profiles WHERE tenant_id = ?1")
            .bind(tenant_id)
            .fetch_one(pool)
            .await
            .map_err(|error| error.to_string())?,
        "navigation_state" => {
            sqlx::query_scalar("SELECT COUNT(*) FROM navigation_state WHERE tenant_id = ?1")
                .bind(tenant_id)
                .fetch_one(pool)
                .await
                .map_err(|error| error.to_string())?
        }
        "app_shortcut_items" => {
            sqlx::query_scalar("SELECT COUNT(*) FROM app_shortcut_items WHERE tenant_id = ?1")
                .bind(tenant_id)
                .fetch_one(pool)
                .await
                .map_err(|error| error.to_string())?
        }
        other => return Err(format!("unsupported count table: {other}")),
    };
    Ok(count as usize)
}

#[cfg(test)]
pub(crate) fn migrate_database(db_path: &Path) -> AppResult<()> {
    let db_path = db_path.to_path_buf();
    std::thread::spawn(move || {
        let runtime = build_runtime()?;
        let pool = runtime.block_on(open_migrated_pool(&db_path))?;
        runtime.block_on(pool.close());
        Ok(())
    })
    .join()
    .map_err(|_| "SQLx migration worker panicked".to_string())?
}

fn build_runtime() -> AppResult<Runtime> {
    tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .map_err(|error| error.to_string())
}

async fn open_migrated_pool(db_path: &Path) -> AppResult<SqlitePool> {
    let options = SqliteConnectOptions::new()
        .filename(db_path)
        .create_if_missing(true)
        .foreign_keys(true)
        .journal_mode(SqliteJournalMode::Wal)
        .synchronous(SqliteSynchronous::Normal)
        .busy_timeout(Duration::from_secs(10));
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await
        .map_err(|error| error.to_string())?;

    if is_untracked_legacy_database(&pool).await? {
        upgrade_legacy_schema(&pool).await?;
    }
    MIGRATOR
        .run(&pool)
        .await
        .map_err(|error| error.to_string())?;
    Ok(pool)
}

async fn seed_defaults_sqlx(pool: &SqlitePool) -> AppResult<()> {
    super::tenant_repo::ensure_local_identity_sqlx(pool).await?;
    let tenant_id = super::tenant_repo::DEFAULT_TENANT_ID;

    seed_tenant_defaults_sqlx(pool, tenant_id).await?;

    Ok(())
}

pub(crate) async fn seed_tenant_defaults_sqlx(pool: &SqlitePool, tenant_id: &str) -> AppResult<()> {
    if count_rows(pool, tenant_id, "sources").await? == 0 {
        for source in crate::backend::defaults::default_sources_for_tenant(tenant_id) {
            super::source_repo::upsert_source_sqlx(pool, tenant_id, &source).await?;
        }
    }
    ensure_library_source_sqlx(pool, tenant_id).await?;
    normalize_existing_sources_sqlx(pool, tenant_id).await?;

    if count_rows(pool, tenant_id, "profiles").await? == 0 {
        for profile in crate::backend::defaults::default_profiles() {
            super::profile_repo::upsert_profile_sqlx(pool, tenant_id, &profile).await?;
        }
    }
    normalize_default_profiles_sqlx(pool, tenant_id).await?;

    super::conversation_repo::seed_builtin_conversation_adapters_sqlx(pool, tenant_id).await?;
    super::conversation_repo::migrate_legacy_conversation_adapter_hashes_sqlx(pool, tenant_id)
        .await?;

    let default_navigation_model = crate::backend::defaults::default_navigation_model();
    if count_rows(pool, tenant_id, "navigation_state").await? == 0 {
        super::menu_repo::seed_navigation_model_sqlx(pool, tenant_id, &default_navigation_model)
            .await?;
    } else {
        super::menu_repo::ensure_navigation_model_items_sqlx(
            pool,
            tenant_id,
            &default_navigation_model,
        )
        .await?;
    }

    if count_rows(pool, tenant_id, "app_shortcut_items").await? == 0 {
        super::shortcut_repo::seed_app_shortcuts_sqlx(
            pool,
            tenant_id,
            &crate::backend::defaults::default_app_shortcuts(),
        )
        .await?;
    }

    Ok(())
}

async fn ensure_library_source_sqlx(pool: &SqlitePool, tenant_id: &str) -> AppResult<()> {
    if super::source_repo::load_source_sqlx(pool, tenant_id, "assetiweave-library-skills")
        .await?
        .is_some()
    {
        return Ok(());
    }
    if let Some(source) = crate::backend::defaults::default_sources_for_tenant(tenant_id)
        .into_iter()
        .find(|source| source.id == "assetiweave-library-skills")
    {
        super::source_repo::upsert_source_sqlx(pool, tenant_id, &source).await?;
    }
    Ok(())
}

async fn normalize_existing_sources_sqlx(pool: &SqlitePool, tenant_id: &str) -> AppResult<()> {
    for source in super::source_repo::load_sources_sqlx(pool, tenant_id).await? {
        super::source_repo::upsert_source_sqlx(pool, tenant_id, &source).await?;
    }
    Ok(())
}

async fn normalize_default_profiles_sqlx(pool: &SqlitePool, tenant_id: &str) -> AppResult<()> {
    let defaults = crate::backend::defaults::default_profiles();
    for mut profile in super::profile_repo::load_profiles_sqlx(pool, tenant_id).await? {
        let Some(default_profile) = defaults.iter().find(|candidate| candidate.id == profile.id)
        else {
            continue;
        };
        if legacy_profile_target_paths(&profile.id).contains(&profile.target_paths) {
            profile.target_paths = default_profile.target_paths.clone();
            super::profile_repo::upsert_profile_sqlx(pool, tenant_id, &profile).await?;
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

async fn is_untracked_legacy_database(pool: &SqlitePool) -> AppResult<bool> {
    Ok(table_exists(pool, "sources").await? && !table_exists(pool, "_sqlx_migrations").await?)
}

async fn table_exists(pool: &SqlitePool, table: &str) -> AppResult<bool> {
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?")
            .bind(table)
            .fetch_one(pool)
            .await
            .map_err(|error| error.to_string())?;
    Ok(count == 1)
}

async fn upgrade_legacy_schema(pool: &SqlitePool) -> AppResult<()> {
    for (table, column, statement) in LEGACY_COLUMN_MIGRATIONS {
        if table_exists(pool, table).await? && !column_exists(pool, table, column).await? {
            sqlx::query(*statement)
                .execute(pool)
                .await
                .map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}

async fn column_exists(pool: &SqlitePool, table: &str, column: &str) -> AppResult<bool> {
    let statement = format!("PRAGMA table_info({table})");
    let rows = sqlx::query(AssertSqlSafe(statement))
        .fetch_all(pool)
        .await
        .map_err(|error| error.to_string())?;
    rows.iter()
        .map(|row| row.try_get::<String, _>("name"))
        .collect::<Result<Vec<_>, _>>()
        .map(|columns| columns.iter().any(|candidate| candidate == column))
        .map_err(|error| error.to_string())
}

const LEGACY_COLUMN_MIGRATIONS: &[(&str, &str, &str)] = &[
    (
        "sources",
        "scanner_kind",
        "ALTER TABLE sources ADD COLUMN scanner_kind TEXT NOT NULL DEFAULT 'mixed'",
    ),
    (
        "sources",
        "source_origin",
        "ALTER TABLE sources ADD COLUMN source_origin TEXT NOT NULL DEFAULT 'local_folder'",
    ),
    (
        "sources",
        "repo_root",
        "ALTER TABLE sources ADD COLUMN repo_root TEXT",
    ),
    (
        "sources",
        "scan_root",
        "ALTER TABLE sources ADD COLUMN scan_root TEXT NOT NULL DEFAULT ''",
    ),
    (
        "sources",
        "origin_app_kind",
        "ALTER TABLE sources ADD COLUMN origin_app_kind TEXT",
    ),
    (
        "rail_menu_items",
        "label_zh",
        "ALTER TABLE rail_menu_items ADD COLUMN label_zh TEXT",
    ),
    (
        "rail_menu_items",
        "label_en",
        "ALTER TABLE rail_menu_items ADD COLUMN label_en TEXT",
    ),
    (
        "header_tab_items",
        "label_zh",
        "ALTER TABLE header_tab_items ADD COLUMN label_zh TEXT",
    ),
    (
        "header_tab_items",
        "label_en",
        "ALTER TABLE header_tab_items ADD COLUMN label_en TEXT",
    ),
    (
        "sub_nav_items",
        "label_zh",
        "ALTER TABLE sub_nav_items ADD COLUMN label_zh TEXT",
    ),
    (
        "sub_nav_items",
        "label_en",
        "ALTER TABLE sub_nav_items ADD COLUMN label_en TEXT",
    ),
    (
        "app_shortcut_items",
        "icon_svg",
        "ALTER TABLE app_shortcut_items ADD COLUMN icon_svg TEXT",
    ),
    (
        "asset_groups",
        "display_icon",
        "ALTER TABLE asset_groups ADD COLUMN display_icon TEXT",
    ),
    (
        "asset_groups",
        "icon_svg",
        "ALTER TABLE asset_groups ADD COLUMN icon_svg TEXT",
    ),
];

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;
    use uuid::Uuid;

    #[test]
    fn migrations_create_fresh_schema_and_track_version() {
        let db_path = temp_database_path("fresh");

        migrate_database(&db_path).expect("run migrations");

        let conn = Connection::open(&db_path).expect("open migrated database");
        let source_table_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = 'sources'",
                [],
                |row| row.get(0),
            )
            .expect("query sources table");
        let migration_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM _sqlx_migrations", [], |row| {
                row.get(0)
            })
            .expect("query migrations");

        assert_eq!(source_table_count, 1);
        assert_eq!(migration_count, 6);
        cleanup_database(&db_path);
    }

    #[test]
    fn migrations_adopt_legacy_schema_without_losing_rows() {
        let db_path = temp_database_path("legacy");
        let conn = Connection::open(&db_path).expect("open legacy database");
        conn.execute_batch(
            r#"
            CREATE TABLE sources (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                kind TEXT NOT NULL,
                root_path TEXT NOT NULL,
                include_globs TEXT NOT NULL,
                exclude_globs TEXT NOT NULL,
                default_kind TEXT,
                enabled INTEGER NOT NULL,
                priority INTEGER NOT NULL,
                last_scanned_at TEXT,
                last_scan_status TEXT
            );
            INSERT INTO sources (
                id, name, kind, root_path, include_globs, exclude_globs,
                default_kind, enabled, priority
            ) VALUES (
                'legacy-source', 'Legacy', 'local', '/tmp/legacy', '[]', '[]',
                NULL, 1, 10
            );
            "#,
        )
        .expect("create legacy schema");
        drop(conn);

        migrate_database(&db_path).expect("adopt legacy database");

        let conn = Connection::open(&db_path).expect("open migrated database");
        let source: (String, String, String) = conn
            .query_row(
                "SELECT id, scanner_kind, source_origin FROM sources WHERE id = 'legacy-source'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .expect("query preserved source");
        let migration_count: i64 = conn
            .query_row("SELECT COUNT(*) FROM _sqlx_migrations", [], |row| {
                row.get(0)
            })
            .expect("query migrations");

        assert_eq!(
            source,
            (
                "legacy-source".to_string(),
                "mixed".to_string(),
                "local_folder".to_string()
            )
        );
        assert_eq!(migration_count, 6);
        cleanup_database(&db_path);
    }

    #[test]
    fn database_reuses_pool_for_queries_after_migration() {
        let db_path = temp_database_path("pool");
        let database = Database::open(&db_path).expect("open database");

        let source_count = database
            .block_on(async {
                sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM sources")
                    .fetch_one(database.pool())
                    .await
            })
            .expect("query via SQLx pool");

        assert_eq!(source_count, 0);
        drop(database);
        cleanup_database(&db_path);
    }

    #[test]
    fn initialized_database_seeds_defaults_without_reseeding() {
        let db_path = temp_database_path("initialized");
        let database = Database::open_initialized(&db_path).expect("open initialized database");

        let (source_count, profile_count, navigation_count, shortcut_count) = database
            .block_on(async {
                let source_count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM sources")
                    .fetch_one(database.pool())
                    .await
                    .map_err(|error| error.to_string())?;
                let profile_count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM profiles")
                    .fetch_one(database.pool())
                    .await
                    .map_err(|error| error.to_string())?;
                let navigation_count =
                    sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM navigation_state")
                        .fetch_one(database.pool())
                        .await
                        .map_err(|error| error.to_string())?;
                let shortcut_count =
                    sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM app_shortcut_items")
                        .fetch_one(database.pool())
                        .await
                        .map_err(|error| error.to_string())?;
                AppResult::Ok((
                    source_count,
                    profile_count,
                    navigation_count,
                    shortcut_count,
                ))
            })
            .expect("query seeded defaults");

        assert!(source_count > 0);
        assert!(profile_count > 0);
        assert_eq!(navigation_count, 1);
        assert!(shortcut_count > 0);
        drop(database);

        let conn = Connection::open(&db_path).expect("open seeded database");
        conn.execute(
            "UPDATE conversation_adapters SET name = 'preserved' WHERE id = 'codex'",
            [],
        )
        .expect("customize seeded adapter");
        drop(conn);

        let reopened = Database::open_initialized(&db_path).expect("reopen initialized database");
        let codex_name = reopened
            .block_on(async {
                sqlx::query_scalar::<_, String>(
                    "SELECT name FROM conversation_adapters WHERE id = 'codex'",
                )
                .fetch_one(reopened.pool())
                .await
                .map_err(|error| error.to_string())
            })
            .expect("query preserved adapter");

        assert_eq!(codex_name, "preserved");
        drop(reopened);
        cleanup_database(&db_path);
    }

    #[test]
    fn initialized_database_seeds_local_principal_and_default_tenant() {
        let db_path = temp_database_path("tenant-identity");
        let database = Database::open_initialized(&db_path).expect("open initialized database");

        let (principal_count, tenant_count, membership_count, active_tenant_id) = database
            .block_on(async {
                let principal_count =
                    sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM principals WHERE id = 'local'")
                        .fetch_one(database.pool())
                        .await
                        .map_err(|error| error.to_string())?;
                let tenant_count =
                    sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM tenants WHERE id = 'default'")
                        .fetch_one(database.pool())
                        .await
                        .map_err(|error| error.to_string())?;
                let membership_count = sqlx::query_scalar::<_, i64>(
                    "SELECT COUNT(*) FROM tenant_memberships WHERE principal_id = 'local' AND tenant_id = 'default' AND role = 'owner'",
                )
                .fetch_one(database.pool())
                .await
                .map_err(|error| error.to_string())?;
                let active_tenant_id = sqlx::query_scalar::<_, String>(
                    "SELECT active_tenant_id FROM tenant_state WHERE principal_id = 'local'",
                )
                .fetch_one(database.pool())
                .await
                .map_err(|error| error.to_string())?;
                AppResult::Ok((
                    principal_count,
                    tenant_count,
                    membership_count,
                    active_tenant_id,
                ))
            })
            .expect("query tenant identity defaults");

        assert_eq!(principal_count, 1);
        assert_eq!(tenant_count, 1);
        assert_eq!(membership_count, 1);
        assert_eq!(active_tenant_id, "default");
        drop(database);
        cleanup_database(&db_path);
    }

    fn temp_database_path(label: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "assetiweave-sqlx-{label}-{}.sqlite",
            Uuid::new_v4()
        ))
    }

    fn cleanup_database(db_path: &std::path::Path) {
        let _ = std::fs::remove_file(db_path);
        let _ = std::fs::remove_file(db_path.with_extension("sqlite-wal"));
        let _ = std::fs::remove_file(db_path.with_extension("sqlite-shm"));
    }
}
