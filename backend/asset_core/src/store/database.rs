use crate::backend::store::{StoreError, StoreResult};
use sqlx::{
    migrate::Migrator,
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous},
    AssertSqlSafe, Row, SqlitePool,
};
use std::{path::Path, time::Duration};

static MIGRATOR: Migrator = sqlx::migrate!("./migrations");

#[derive(Clone)]
pub struct Database {
    pool: SqlitePool,
}

impl Database {
    pub fn from_pool(pool: SqlitePool) -> Self {
        Self { pool }
    }

    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    #[cfg(test)]
    pub async fn open_async(db_path: &Path) -> StoreResult<Self> {
        let pool = open_migrated_pool(db_path).await?;
        Ok(Self::from_pool(pool))
    }

    /// 开启 SQLite 原生立即写事务（`BEGIN IMMEDIATE`）。
    pub async fn begin_immediate(&self) -> StoreResult<sqlx::Transaction<'static, sqlx::Sqlite>> {
        begin_immediate(&self.pool).await
    }
}

/// 显式开启 SQLite 立即写事务（`BEGIN IMMEDIATE`）。
///
/// 在 WAL 模式下，默认 `BEGIN`（即 `BEGIN DEFERRED`）在先读后写时会遭遇快照过期与锁升级失败
/// （`SQLITE_BUSY_SNAPSHOT (code: 517)`）。使用本函数可在事务第一条语句即申请 RESERVED 写锁，
/// 锁定最新快照，彻底根除锁升级引发的快照冲突。
pub async fn begin_immediate(
    pool: &SqlitePool,
) -> StoreResult<sqlx::Transaction<'static, sqlx::Sqlite>> {
    let conn = pool.acquire().await.map_err(StoreError::external)?;
    sqlx::Transaction::begin(
        conn,
        Some(sqlx::SqlSafeStr::into_sql_str("BEGIN IMMEDIATE")),
    )
    .await
    .map_err(StoreError::external)
}

pub async fn latest_scan_status(pool: &SqlitePool, tenant_id: &str) -> StoreResult<String> {
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

pub async fn count_rows(pool: &SqlitePool, tenant_id: &str, table: &str) -> StoreResult<usize> {
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
            )));
        }
    };
    Ok(count as usize)
}

#[cfg(test)]
pub async fn migrate_database(db_path: &Path) -> StoreResult<()> {
    let pool = open_migrated_pool(db_path).await?;
    pool.close().await;
    Ok(())
}

pub async fn open_migrated_pool(db_path: &Path) -> StoreResult<SqlitePool> {
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
    repair_legacy_migration_ledger_versions(&pool).await?;
    repair_known_modified_conversation_question_contract_migration(&pool).await?;
    repair_known_modified_cleanup_util6_orphans_migration(&pool).await?;
    repair_superseded_session_snapshot_migration(&pool).await?;
    normalize_legacy_part_ids(&pool).await?;
    MIGRATOR.run(&pool).await.map_err(StoreError::external)?;
    // 迁移脚本算不出 SHA-256，轮次位置主键必须由 Rust 在迁移后就地重算并级联 Parts / FTS / 父引用。
    crate::backend::store::conversation_turn_identity::repair_positional_turn_identities(&pool)
        .await?;
    converge_asset_stored_paths(&pool).await?;
    Ok(pool)
}

/// 把历史遗留的主机绝对路径回填为便携锚点。
///
/// 迁移脚本是纯 SQL，拿不到当前主机的 home / 系统数据目录，因此锚点回填放在启动阶段完成。
/// 幂等：锚点形式的路径经 `normalize_path_for_storage` 归一后与原值相同，不会产生任何写入；
/// 无法折叠（不在用户目录与系统数据目录之下）的路径保持原样，跨平台时由调用方降级提示。
async fn converge_asset_stored_paths(pool: &SqlitePool) -> StoreResult<()> {
    if !table_exists(pool, "assets").await? {
        return Ok(());
    }

    let rows = sqlx::query("SELECT tenant_id, id, stored_path FROM assets")
        .fetch_all(pool)
        .await
        .map_err(StoreError::external)?;

    let mut pending = Vec::new();
    for row in &rows {
        let tenant_id: String = row.try_get(0).map_err(StoreError::external)?;
        let asset_id: String = row.try_get(1).map_err(StoreError::external)?;
        let stored_path: String = row.try_get(2).map_err(StoreError::external)?;
        let Ok(portable) =
            crate::backend::store::path_codec::normalize_path_for_storage(&stored_path)
        else {
            continue;
        };
        if portable != stored_path {
            pending.push((tenant_id, asset_id, portable));
        }
    }

    if pending.is_empty() {
        return Ok(());
    }

    let mut tx = pool.begin().await.map_err(StoreError::external)?;
    for (tenant_id, asset_id, portable) in pending {
        sqlx::query("UPDATE assets SET stored_path = ?1 WHERE tenant_id = ?2 AND id = ?3")
            .bind(&portable)
            .bind(&tenant_id)
            .bind(&asset_id)
            .execute(&mut *tx)
            .await
            .map_err(StoreError::external)?;
    }
    tx.commit().await.map_err(StoreError::external)?;
    Ok(())
}

async fn is_untracked_legacy_database(pool: &SqlitePool) -> StoreResult<bool> {
    Ok(table_exists(pool, "sources").await? && !table_exists(pool, "_sqlx_migrations").await?)
}

pub(crate) async fn table_exists(pool: &SqlitePool, table: &str) -> StoreResult<bool> {
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

async fn repair_legacy_migration_ledger_versions(pool: &SqlitePool) -> StoreResult<()> {
    if !table_exists(pool, "_sqlx_migrations").await? {
        return Ok(());
    }
    sqlx::query(
        "UPDATE _sqlx_migrations \
         SET version = (version / 10000) * 1000000 + (version % 10000) \
         WHERE version < 10000000000000",
    )
    .execute(pool)
    .await
    .map_err(StoreError::external)?;
    Ok(())
}

async fn repair_known_modified_conversation_question_contract_migration(
    pool: &SqlitePool,
) -> StoreResult<()> {
    if !table_exists(pool, "_sqlx_migrations").await? {
        return Ok(());
    }
    let checksum: Option<String> = sqlx::query_scalar(
        "SELECT upper(hex(checksum)) FROM _sqlx_migrations WHERE version = 20260825000005",
    )
    .fetch_optional(pool)
    .await
    .map_err(StoreError::external)?;
    if checksum.as_deref() != Some(KNOWN_MODIFIED_QUESTION_CONTRACT_CHECKSUM) {
        return Ok(());
    }

    for table in ["conversation_questions", "web_record_questions"] {
        if table_exists(pool, table).await? {
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
                        "cannot repair migration 20260825000005 checksum: {table}.{removed_column} still exists"
                    )));
                }
            }
        }
    }

    sqlx::query(
        "UPDATE _sqlx_migrations SET checksum = X'85BFCAA1EDBB892E90FB943086A77885E6A6C7D349106049CA71A43F9655A60682B18A9D69CCE576EEA3CCDCEE1E0CF2' WHERE version = 20260825000005",
    )
    .execute(pool)
    .await
    .map_err(StoreError::external)?;
    Ok(())
}

const KNOWN_MODIFIED_QUESTION_CONTRACT_CHECKSUM: &str = "E9A07424F1C86E99CED78966A8CB750B73D6C00E089576DD5C077B357B259BE2C34C14F0BEC320A2FB991B98B6BB9D38";

const KNOWN_MODIFIED_CLEANUP_UTIL6_ORPHANS_CHECKSUM: &str = "B45D2395822AF941B7760B9F39EC3E2ED35B8101540955EE01D219895637506A553E8620D57E48BD30BB66B5943FD928";

/// `20261106000001` 首版把「轮次位置映射」写成了内连接，会话行缺失的孤儿轮次会在
/// 重建时被静默丢弃；修正版改成左连接。已经应用过首版的库表结构完全正确、无需重跑，
/// 只是账本里的校验和还是旧文件的，需要在 `MIGRATOR.run` 之前先对齐，否则启动直接 panic。
const SUPERSEDED_SESSION_SNAPSHOT_CHECKSUM: &str = "0873471DAC6AB5BF57D94A48B78688DF7F91D6CF14A615E63664D3A79476319D7FC7C5B0F9AF64C3F68F0738BE572177";

const SESSION_SNAPSHOT_MIGRATION_VERSION: i64 = 20261106000001;

/// 把被修正过的 `20261106000001` 账本校验和对齐到当前文件。
///
/// 只认那一份确切的旧校验和，并且先确认库确实已经是 11 列位置身份结构：
/// 结构对不上就报错退出，绝不用校验和改写去掩盖一个真正不同的迁移文件。
async fn repair_superseded_session_snapshot_migration(pool: &SqlitePool) -> StoreResult<()> {
    if !table_exists(pool, "_sqlx_migrations").await? {
        return Ok(());
    }
    let checksum: Option<String> =
        sqlx::query_scalar("SELECT upper(hex(checksum)) FROM _sqlx_migrations WHERE version = ?1")
            .bind(SESSION_SNAPSHOT_MIGRATION_VERSION)
            .fetch_optional(pool)
            .await
            .map_err(StoreError::external)?;
    if checksum.as_deref() != Some(SUPERSEDED_SESSION_SNAPSHOT_CHECKSUM) {
        return Ok(());
    }

    for table in ["conversation_turns", "web_record_turns"] {
        if column_exists(pool, table, "external_id").await?
            || !column_exists(pool, table, "turn_index").await?
        {
            return Err(StoreError::Validation(format!(
                "cannot repair migration {SESSION_SNAPSHOT_MIGRATION_VERSION} checksum: {table} is not in the 11-column positional shape"
            )));
        }
    }
    if !column_exists(pool, "conversation_usage_events", "turn_index").await? {
        return Err(StoreError::Validation(format!(
            "cannot repair migration {SESSION_SNAPSHOT_MIGRATION_VERSION} checksum: conversation_usage_events.turn_index is missing"
        )));
    }

    let Some(current) = MIGRATOR
        .iter()
        .find(|migration| migration.version == SESSION_SNAPSHOT_MIGRATION_VERSION)
    else {
        return Ok(());
    };
    sqlx::query("UPDATE _sqlx_migrations SET checksum = ?1 WHERE version = ?2")
        .bind(current.checksum.to_vec())
        .bind(SESSION_SNAPSHOT_MIGRATION_VERSION)
        .execute(pool)
        .await
        .map_err(StoreError::external)?;
    Ok(())
}

async fn repair_known_modified_cleanup_util6_orphans_migration(
    pool: &SqlitePool,
) -> StoreResult<()> {
    if !table_exists(pool, "_sqlx_migrations").await? {
        return Ok(());
    }
    let checksum: Option<String> = sqlx::query_scalar(
        "SELECT upper(hex(checksum)) FROM _sqlx_migrations WHERE version = 20261024000005",
    )
    .fetch_optional(pool)
    .await
    .map_err(StoreError::external)?;

    if checksum.as_deref() != Some(KNOWN_MODIFIED_CLEANUP_UTIL6_ORPHANS_CHECKSUM) {
        return Ok(());
    }

    let mut transaction = pool.begin().await.map_err(StoreError::external)?;

    // 自愈被旧版 0005 误伤的正常命令零件
    sqlx::query(
        "UPDATE conversation_parts \
         SET attrs_json = json_remove(attrs_json, '$.has_command') \
         WHERE kind = 'command' \
            AND json_extract(attrs_json, '$.has_command') = 0 \
            AND content IS NOT NULL",
    )
    .execute(&mut *transaction)
    .await
    .map_err(StoreError::external)?;

    // 更新 checksum 为修正后的 0005 迁移 SHA384
    sqlx::query(
        "UPDATE _sqlx_migrations \
         SET checksum = X'9C3FE1960034A1516BDDAEC65B2C5C0E25D78B56193B4631E1AA07BA19600CE8B5E845FAF77BC7C4C5117EB1C861B2D7' \
         WHERE version = 20261024000005",
    )
    .execute(&mut *transaction)
    .await
    .map_err(StoreError::external)?;

    transaction.commit().await.map_err(StoreError::external)?;
    Ok(())
}

pub(crate) async fn column_exists(
    pool: &SqlitePool,
    table: &str,
    column: &str,
) -> StoreResult<bool> {
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

async fn normalize_legacy_part_ids(pool: &SqlitePool) -> StoreResult<()> {
    if !table_exists(pool, "conversation_parts").await? {
        return Ok(());
    }
    if !column_exists(pool, "conversation_parts", "id").await? {
        return Ok(());
    }

    let legacy_parts: Vec<(String, String, String, i64)> = sqlx::query_as(
        "SELECT tenant_id, id, turn_id, part_index FROM conversation_parts WHERE id NOT LIKE 'conversation-part-%'",
    )
    .fetch_all(pool)
    .await
    .map_err(StoreError::external)?;

    if !legacy_parts.is_empty() {
        let mut tx = pool.begin().await.map_err(StoreError::external)?;
        for (tenant_id, old_id, turn_id, part_index) in legacy_parts {
            let new_id =
                crate::backend::domain::derive_part_id("conversation-part", &turn_id, part_index);
            sqlx::query("UPDATE conversation_parts SET id = ?1 WHERE tenant_id = ?2 AND id = ?3")
                .bind(&new_id)
                .bind(&tenant_id)
                .bind(&old_id)
                .execute(&mut *tx)
                .await
                .map_err(StoreError::external)?;

            if table_exists(pool, "conversation_part_fts").await? {
                sqlx::query(
                    "UPDATE conversation_part_fts SET part_id = ?1 WHERE tenant_id = ?2 AND part_id = ?3",
                )
                .bind(&new_id)
                .bind(&tenant_id)
                .bind(&old_id)
                .execute(&mut *tx)
                .await
                .map_err(StoreError::external)?;
            }
        }
        tx.commit().await.map_err(StoreError::external)?;
    }

    if table_exists(pool, "web_record_parts").await?
        && column_exists(pool, "web_record_parts", "id").await?
    {
        let legacy_web_parts: Vec<(String, String, String, i64)> = sqlx::query_as(
            "SELECT tenant_id, id, turn_id, part_index FROM web_record_parts WHERE id NOT LIKE 'web-record-part-%'",
        )
        .fetch_all(pool)
        .await
        .map_err(StoreError::external)?;

        if !legacy_web_parts.is_empty() {
            let mut tx = pool.begin().await.map_err(StoreError::external)?;
            for (tenant_id, old_id, turn_id, part_index) in legacy_web_parts {
                let new_id =
                    crate::backend::domain::derive_part_id("web-record-part", &turn_id, part_index);
                sqlx::query("UPDATE web_record_parts SET id = ?1 WHERE tenant_id = ?2 AND id = ?3")
                    .bind(&new_id)
                    .bind(&tenant_id)
                    .bind(&old_id)
                    .execute(&mut *tx)
                    .await
                    .map_err(StoreError::external)?;

                if table_exists(pool, "conversation_part_fts").await? {
                    sqlx::query(
                        "UPDATE conversation_part_fts SET part_id = ?1 WHERE tenant_id = ?2 AND part_id = ?3",
                    )
                    .bind(&new_id)
                    .bind(&tenant_id)
                    .bind(&old_id)
                    .execute(&mut *tx)
                    .await
                    .map_err(StoreError::external)?;
                }
            }
            tx.commit().await.map_err(StoreError::external)?;
        }
    }

    Ok(())
}

#[cfg(test)]
#[path = "database_tests.rs"]
mod tests;
