use crate::backend::store::{StoreError, StoreResult};
use sqlx::{
    migrate::Migrator,
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous},
    AssertSqlSafe, Row, SqlitePool,
};
use std::{path::Path, time::Duration};

static MIGRATOR: Migrator = sqlx::migrate!("./migrations");

#[derive(Clone)]
pub(crate) struct Database {
    pool: SqlitePool,
}

impl Database {
    pub(crate) fn from_pool(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub(crate) fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    #[cfg(test)]
    pub(crate) async fn open_async(db_path: &Path) -> StoreResult<Self> {
        let pool = open_migrated_pool(db_path).await?;
        Ok(Self::from_pool(pool))
    }
}

pub(crate) async fn latest_scan_status(pool: &SqlitePool, tenant_id: &str) -> StoreResult<String> {
    let status: Option<String> = sqlx::query_scalar(
        "SELECT last_scan_status FROM sources WHERE tenant_id = ?1 ORDER BY last_scanned_at DESC NULLS LAST LIMIT 1",
    )
    .bind(tenant_id)
    .fetch_optional(pool)
    .await
    .map_err(StoreError::external)?
    .flatten();
    Ok(status.unwrap_or_else(|| "等待首次扫描".to_string()))
}

pub(crate) async fn count_rows(
    pool: &SqlitePool,
    tenant_id: &str,
    table: &str,
) -> StoreResult<usize> {
    let count: i64 = match table {
        "sources" => sqlx::query_scalar("SELECT COUNT(*) FROM sources WHERE tenant_id = ?1")
            .bind(tenant_id)
            .fetch_one(pool)
            .await
            .map_err(StoreError::external)?,
        "assets" => sqlx::query_scalar("SELECT COUNT(*) FROM assets WHERE tenant_id = ?1")
            .bind(tenant_id)
            .fetch_one(pool)
            .await
            .map_err(StoreError::external)?,
        "profiles" => sqlx::query_scalar("SELECT COUNT(*) FROM profiles WHERE tenant_id = ?1")
            .bind(tenant_id)
            .fetch_one(pool)
            .await
            .map_err(StoreError::external)?,
        "navigation_state" => {
            sqlx::query_scalar("SELECT COUNT(*) FROM navigation_state WHERE tenant_id = ?1")
                .bind(tenant_id)
                .fetch_one(pool)
                .await
                .map_err(StoreError::external)?
        }
        "app_shortcut_items" => {
            sqlx::query_scalar("SELECT COUNT(*) FROM app_shortcut_items WHERE tenant_id = ?1")
                .bind(tenant_id)
                .fetch_one(pool)
                .await
                .map_err(StoreError::external)?
        }
        other => {
            return Err(StoreError::Validation(format!(
                "unsupported count table: {other}"
            )))
        }
    };
    Ok(count as usize)
}

#[cfg(test)]
pub(crate) async fn migrate_database(db_path: &Path) -> StoreResult<()> {
    let pool = open_migrated_pool(db_path).await?;
    pool.close().await;
    Ok(())
}

pub(crate) async fn open_migrated_pool(db_path: &Path) -> StoreResult<SqlitePool> {
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
        .map_err(StoreError::external)?;

    if is_untracked_legacy_database(&pool).await? {
        upgrade_legacy_schema(&pool).await?;
    }
    repair_known_modified_catalog_release_migration(&pool).await?;
    repair_known_modified_conversation_question_contract_migration(&pool).await?;
    MIGRATOR.run(&pool).await.map_err(StoreError::external)?;
    Ok(pool)
}

async fn is_untracked_legacy_database(pool: &SqlitePool) -> StoreResult<bool> {
    Ok(table_exists(pool, "sources").await? && !table_exists(pool, "_sqlx_migrations").await?)
}

async fn table_exists(pool: &SqlitePool, table: &str) -> StoreResult<bool> {
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = ?")
            .bind(table)
            .fetch_one(pool)
            .await
            .map_err(StoreError::external)?;
    Ok(count == 1)
}

async fn upgrade_legacy_schema(pool: &SqlitePool) -> StoreResult<()> {
    for (table, column, statement) in LEGACY_COLUMN_MIGRATIONS {
        if table_exists(pool, table).await? && !column_exists(pool, table, column).await? {
            sqlx::query(*statement)
                .execute(pool)
                .await
                .map_err(StoreError::external)?;
        }
    }
    Ok(())
}

async fn repair_known_modified_catalog_release_migration(pool: &SqlitePool) -> StoreResult<()> {
    if !table_exists(pool, "_sqlx_migrations").await? {
        return Ok(());
    }
    let checksum: Option<String> = sqlx::query_scalar(
        "SELECT upper(hex(checksum)) FROM _sqlx_migrations WHERE version = 202607150002",
    )
    .fetch_optional(pool)
    .await
    .map_err(StoreError::external)?;
    if checksum.as_deref() != Some(KNOWN_MODIFIED_CATALOG_RELEASE_DETAILS_CHECKSUM) {
        return Ok(());
    }
    if !column_exists(pool, "conversation_adapter_catalog_releases", "source_json").await? {
        return Ok(());
    }

    let mut transaction = pool.begin().await.map_err(StoreError::external)?;
    sqlx::query(
        "UPDATE _sqlx_migrations SET checksum = X'E75F75A803B17920C2E1498962D6DCB8A34167DE66AAB216FBC2F4CA056A383E0600CAEFE44633CDA48A0F9FE61DC581' WHERE version = 202607150002",
    )
    .execute(&mut *transaction)
    .await
    .map_err(StoreError::external)?;
    sqlx::query(
        "INSERT OR IGNORE INTO _sqlx_migrations (version, description, success, checksum, execution_time) VALUES (202607160001, 'conversation adapter catalog release source', 1, X'39DC30B774A3C414B1AC69B49F0036E713F2B75A973B8F75A5E796064373FE4170FC9E2B3305509DF928C85A26C5736E', 0)",
    )
    .execute(&mut *transaction)
    .await
    .map_err(StoreError::external)?;
    transaction.commit().await.map_err(StoreError::external)
}

const KNOWN_MODIFIED_CATALOG_RELEASE_DETAILS_CHECKSUM: &str =
    "863286E8B8E292E94EB63E42AC751D8EAD340500965B16ADCB4EEF61BEB38187B9E8FF4F19379B7C817F18DDD3BF0A83";

async fn repair_known_modified_conversation_question_contract_migration(
    pool: &SqlitePool,
) -> StoreResult<()> {
    if !table_exists(pool, "_sqlx_migrations").await? {
        return Ok(());
    }
    let checksum: Option<String> = sqlx::query_scalar(
        "SELECT upper(hex(checksum)) FROM _sqlx_migrations WHERE version = 202608250005",
    )
    .fetch_optional(pool)
    .await
    .map_err(StoreError::external)?;
    if checksum.as_deref() != Some(KNOWN_MODIFIED_QUESTION_CONTRACT_CHECKSUM) {
        return Ok(());
    }

    for table in ["conversation_questions", "web_record_questions"] {
        if !table_exists(pool, table).await? {
            return Err(StoreError::Validation(format!(
                "cannot repair migration 202608250005 checksum: missing {table}"
            )));
        }
        for removed_column in [
            "question_index",
            "question_text",
            "answer_text",
            "code_text",
            "command_text",
            "grouping_origin",
        ] {
            if column_exists(pool, table, removed_column).await? {
                return Err(StoreError::Validation(format!(
                    "cannot repair migration 202608250005 checksum: {table}.{removed_column} still exists"
                )));
            }
        }
    }

    sqlx::query(
        "UPDATE _sqlx_migrations SET checksum = X'85BFCAA1EDBB892E90FB943086A77885E6A6C7D349106049CA71A43F9655A60682B18A9D69CCE576EEA3CCDCEE1E0CF2' WHERE version = 202608250005",
    )
    .execute(pool)
    .await
    .map_err(StoreError::external)?;
    Ok(())
}

const KNOWN_MODIFIED_QUESTION_CONTRACT_CHECKSUM: &str =
    "E9A07424F1C86E99CED78966A8CB750B73D6C00E089576DD5C077B357B259BE2C34C14F0BEC320A2FB991B98B6BB9D38";

async fn column_exists(pool: &SqlitePool, table: &str, column: &str) -> StoreResult<bool> {
    let statement = format!("PRAGMA table_info({table})");
    let rows = sqlx::query(AssertSqlSafe(statement))
        .fetch_all(pool)
        .await
        .map_err(StoreError::external)?;
    rows.iter()
        .map(|row| row.try_get::<String, _>("name"))
        .collect::<Result<Vec<_>, _>>()
        .map(|columns| columns.iter().any(|candidate| candidate == column))
        .map_err(StoreError::external)
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
#[path = "database_tests.rs"]
mod tests;
