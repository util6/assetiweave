use crate::backend::dto::AppResult;
use sqlx::{
    migrate::Migrator,
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous},
    AssertSqlSafe, Row, SqlitePool,
};
use std::{path::Path, time::Duration};

static MIGRATOR: Migrator = sqlx::migrate!("./migrations");

pub(crate) fn migrate_database(db_path: &Path) -> AppResult<()> {
    let db_path = db_path.to_path_buf();
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .map_err(|error| error.to_string())?;
        runtime.block_on(migrate_database_async(&db_path))
    })
    .join()
    .map_err(|_| "SQLx migration worker panicked".to_string())?
}

async fn migrate_database_async(db_path: &Path) -> AppResult<()> {
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
    pool.close().await;
    Ok(())
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
        assert_eq!(migration_count, 1);
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
        assert_eq!(migration_count, 1);
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
