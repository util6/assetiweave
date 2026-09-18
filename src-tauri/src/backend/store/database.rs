#[cfg(test)]
use crate::backend::path_utils::ensure_app_library_dirs;
use crate::backend::runtime::{AppError, AppResult};
use crate::backend::target_catalog::TargetCatalog;
use sqlx::{
    migrate::Migrator,
    sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions, SqliteSynchronous},
    AssertSqlSafe, Row, SqlitePool,
};
#[cfg(test)]
use std::{
    collections::BTreeSet,
    path::PathBuf,
    sync::{Mutex, OnceLock},
};
use std::{path::Path, time::Duration};

static MIGRATOR: Migrator = sqlx::migrate!("./migrations");
#[cfg(test)]
static INITIALIZED_DB_PATHS: OnceLock<Mutex<BTreeSet<PathBuf>>> = OnceLock::new();

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
    pub(crate) async fn open_async(db_path: &Path) -> AppResult<Self> {
        let pool = open_migrated_pool(db_path).await?;
        Ok(Self::from_pool(pool))
    }

    #[cfg(test)]
    pub(crate) async fn open_initialized_async(db_path: &Path) -> AppResult<Self> {
        let pool = open_migrated_pool(db_path).await?;
        let initialized_paths = INITIALIZED_DB_PATHS.get_or_init(|| Mutex::new(BTreeSet::new()));
        let mut initialized_paths = initialized_paths.lock().map_err(AppError::external)?;
        if !initialized_paths.contains(db_path) {
            ensure_app_library_dirs()?;
            seed_defaults_sqlx(&pool).await?;
            let adapters = crate::backend::conversations::ensure_official_conversation_adapters()?;
            super::conversation_repo::seed_prepared_builtin_conversation_adapters_sqlx(
                &pool,
                super::tenant_repo::DEFAULT_TENANT_ID,
                adapters,
            )
            .await?;
            initialized_paths.insert(db_path.to_path_buf());
        }
        Ok(Self::from_pool(pool))
    }
}

pub(crate) async fn latest_scan_status(pool: &SqlitePool, tenant_id: &str) -> AppResult<String> {
    let status: Option<String> = sqlx::query_scalar(
        "SELECT last_scan_status FROM sources WHERE tenant_id = ?1 ORDER BY last_scanned_at DESC NULLS LAST LIMIT 1",
    )
    .bind(tenant_id)
    .fetch_optional(pool)
    .await
    .map_err(AppError::external)?
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
            .map_err(AppError::external)?,
        "assets" => sqlx::query_scalar("SELECT COUNT(*) FROM assets WHERE tenant_id = ?1")
            .bind(tenant_id)
            .fetch_one(pool)
            .await
            .map_err(AppError::external)?,
        "profiles" => sqlx::query_scalar("SELECT COUNT(*) FROM profiles WHERE tenant_id = ?1")
            .bind(tenant_id)
            .fetch_one(pool)
            .await
            .map_err(AppError::external)?,
        "navigation_state" => {
            sqlx::query_scalar("SELECT COUNT(*) FROM navigation_state WHERE tenant_id = ?1")
                .bind(tenant_id)
                .fetch_one(pool)
                .await
                .map_err(AppError::external)?
        }
        "app_shortcut_items" => {
            sqlx::query_scalar("SELECT COUNT(*) FROM app_shortcut_items WHERE tenant_id = ?1")
                .bind(tenant_id)
                .fetch_one(pool)
                .await
                .map_err(AppError::external)?
        }
        other => {
            return Err(AppError::Validation(format!(
                "unsupported count table: {other}"
            )))
        }
    };
    Ok(count as usize)
}

#[cfg(test)]
pub(crate) async fn migrate_database(db_path: &Path) -> AppResult<()> {
    let pool = open_migrated_pool(db_path).await?;
    pool.close().await;
    Ok(())
}

pub(crate) async fn open_migrated_pool(db_path: &Path) -> AppResult<SqlitePool> {
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
        .map_err(AppError::external)?;

    if is_untracked_legacy_database(&pool).await? {
        upgrade_legacy_schema(&pool).await?;
    }
    repair_known_modified_catalog_release_migration(&pool).await?;
    repair_known_modified_conversation_question_contract_migration(&pool).await?;
    MIGRATOR.run(&pool).await.map_err(AppError::external)?;
    Ok(pool)
}

#[cfg(test)]
pub(crate) async fn seed_defaults_sqlx(pool: &SqlitePool) -> AppResult<()> {
    let catalog = TargetCatalog::builtin()?;
    seed_defaults_sqlx_with_catalog(pool, &catalog).await
}

pub(crate) async fn seed_defaults_sqlx_with_catalog(
    pool: &SqlitePool,
    catalog: &TargetCatalog,
) -> AppResult<()> {
    super::tenant_repo::ensure_local_identity_sqlx(pool).await?;
    let tenant_id = super::tenant_repo::DEFAULT_TENANT_ID;

    seed_tenant_defaults_sqlx_with_catalog(pool, tenant_id, catalog).await?;
    normalize_all_tenant_paths_sqlx(pool).await?;

    Ok(())
}

#[cfg(test)]
pub(crate) async fn seed_tenant_defaults_sqlx(pool: &SqlitePool, tenant_id: &str) -> AppResult<()> {
    let catalog = TargetCatalog::builtin()?;
    seed_tenant_defaults_sqlx_with_catalog(pool, tenant_id, &catalog).await
}

pub(crate) async fn seed_tenant_defaults_sqlx_with_catalog(
    pool: &SqlitePool,
    tenant_id: &str,
    catalog: &TargetCatalog,
) -> AppResult<()> {
    if count_rows(pool, tenant_id, "sources").await? == 0 {
        for source in crate::backend::defaults::default_sources_for_tenant(tenant_id) {
            super::source_repo::upsert_source_sqlx(pool, tenant_id, &source).await?;
        }
    }
    ensure_library_source_sqlx(pool, tenant_id).await?;
    ensure_system_skill_source_sqlx(pool, tenant_id).await?;
    normalize_existing_sources_sqlx(pool, tenant_id).await?;

    if count_rows(pool, tenant_id, "profiles").await? == 0 {
        for profile in crate::backend::defaults::default_profiles_from_catalog(catalog) {
            super::profile_repo::upsert_profile_sqlx(pool, tenant_id, &profile).await?;
        }
    } else {
        ensure_default_profiles_sqlx(pool, tenant_id, catalog).await?;
    }
    normalize_existing_profiles_sqlx(pool, tenant_id).await?;
    normalize_default_profiles_sqlx(pool, tenant_id, catalog).await?;

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
    } else {
        super::shortcut_repo::ensure_default_app_shortcuts_sqlx(
            pool,
            tenant_id,
            &crate::backend::defaults::default_app_shortcuts(),
        )
        .await?;
    }

    Ok(())
}

async fn ensure_default_profiles_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    catalog: &TargetCatalog,
) -> AppResult<()> {
    let existing_profiles = super::profile_repo::load_profiles_sqlx(pool, tenant_id).await?;
    for profile in crate::backend::defaults::default_profiles_from_catalog(catalog) {
        if existing_profiles
            .iter()
            .any(|existing| existing.id == profile.id)
        {
            continue;
        }
        super::profile_repo::upsert_profile_sqlx(pool, tenant_id, &profile).await?;
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

async fn ensure_system_skill_source_sqlx(pool: &SqlitePool, tenant_id: &str) -> AppResult<()> {
    let source = crate::backend::builtin_skills::system_skill_source()?;
    Ok(super::source_repo::upsert_source_sqlx(pool, tenant_id, &source).await?)
}

async fn normalize_existing_sources_sqlx(pool: &SqlitePool, tenant_id: &str) -> AppResult<()> {
    for source in super::source_repo::load_sources_sqlx(pool, tenant_id).await? {
        super::source_repo::upsert_source_sqlx(pool, tenant_id, &source).await?;
    }
    Ok(())
}

async fn normalize_existing_profiles_sqlx(pool: &SqlitePool, tenant_id: &str) -> AppResult<()> {
    for profile in super::profile_repo::load_profiles_sqlx(pool, tenant_id).await? {
        super::profile_repo::upsert_profile_sqlx(pool, tenant_id, &profile).await?;
    }
    Ok(())
}

async fn normalize_all_tenant_paths_sqlx(pool: &SqlitePool) -> AppResult<()> {
    let tenant_ids = sqlx::query_scalar::<_, String>("SELECT id FROM tenants")
        .fetch_all(pool)
        .await
        .map_err(AppError::external)?;
    for tenant_id in tenant_ids {
        if tenant_id == super::tenant_repo::DEFAULT_TENANT_ID {
            continue;
        }
        normalize_existing_sources_sqlx(pool, &tenant_id).await?;
        normalize_existing_profiles_sqlx(pool, &tenant_id).await?;
        super::conversation_repo::normalize_conversation_paths_sqlx(pool, &tenant_id).await?;
    }
    Ok(())
}

async fn normalize_default_profiles_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    catalog: &TargetCatalog,
) -> AppResult<()> {
    let defaults = crate::backend::defaults::default_profiles_from_catalog(catalog);
    let previous_defaults =
        crate::backend::defaults::default_profiles_from_catalog(&TargetCatalog::builtin()?);
    for mut profile in super::profile_repo::load_profiles_sqlx(pool, tenant_id).await? {
        let Some(default_profile) = defaults.iter().find(|candidate| candidate.id == profile.id)
        else {
            continue;
        };
        let matches_previous_catalog = previous_defaults
            .iter()
            .find(|candidate| candidate.id == profile.id)
            .is_some_and(|candidate| candidate.target_paths == profile.target_paths);
        if matches_previous_catalog
            || legacy_profile_target_paths(&profile.id).contains(&profile.target_paths)
        {
            profile.target_provider_id = default_profile.target_provider_id.clone();
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
            .map_err(AppError::external)?;
    Ok(count == 1)
}

async fn upgrade_legacy_schema(pool: &SqlitePool) -> AppResult<()> {
    for (table, column, statement) in LEGACY_COLUMN_MIGRATIONS {
        if table_exists(pool, table).await? && !column_exists(pool, table, column).await? {
            sqlx::query(*statement)
                .execute(pool)
                .await
                .map_err(AppError::external)?;
        }
    }
    Ok(())
}

async fn repair_known_modified_catalog_release_migration(pool: &SqlitePool) -> AppResult<()> {
    if !table_exists(pool, "_sqlx_migrations").await? {
        return Ok(());
    }
    let checksum: Option<String> = sqlx::query_scalar(
        "SELECT upper(hex(checksum)) FROM _sqlx_migrations WHERE version = 202607150002",
    )
    .fetch_optional(pool)
    .await
    .map_err(AppError::external)?;
    if checksum.as_deref() != Some(KNOWN_MODIFIED_CATALOG_RELEASE_DETAILS_CHECKSUM) {
        return Ok(());
    }
    if !column_exists(pool, "conversation_adapter_catalog_releases", "source_json").await? {
        return Ok(());
    }

    let mut transaction = pool.begin().await.map_err(AppError::external)?;
    sqlx::query(
        "UPDATE _sqlx_migrations SET checksum = X'E75F75A803B17920C2E1498962D6DCB8A34167DE66AAB216FBC2F4CA056A383E0600CAEFE44633CDA48A0F9FE61DC581' WHERE version = 202607150002",
    )
    .execute(&mut *transaction)
    .await
    .map_err(AppError::external)?;
    sqlx::query(
        "INSERT OR IGNORE INTO _sqlx_migrations (version, description, success, checksum, execution_time) VALUES (202607160001, 'conversation adapter catalog release source', 1, X'39DC30B774A3C414B1AC69B49F0036E713F2B75A973B8F75A5E796064373FE4170FC9E2B3305509DF928C85A26C5736E', 0)",
    )
    .execute(&mut *transaction)
    .await
    .map_err(AppError::external)?;
    transaction.commit().await.map_err(AppError::external)
}

const KNOWN_MODIFIED_CATALOG_RELEASE_DETAILS_CHECKSUM: &str =
    "863286E8B8E292E94EB63E42AC751D8EAD340500965B16ADCB4EEF61BEB38187B9E8FF4F19379B7C817F18DDD3BF0A83";

async fn repair_known_modified_conversation_question_contract_migration(
    pool: &SqlitePool,
) -> AppResult<()> {
    if !table_exists(pool, "_sqlx_migrations").await? {
        return Ok(());
    }
    let checksum: Option<String> = sqlx::query_scalar(
        "SELECT upper(hex(checksum)) FROM _sqlx_migrations WHERE version = 202608250005",
    )
    .fetch_optional(pool)
    .await
    .map_err(AppError::external)?;
    if checksum.as_deref() != Some(KNOWN_MODIFIED_QUESTION_CONTRACT_CHECKSUM) {
        return Ok(());
    }

    for table in ["conversation_questions", "web_record_questions"] {
        if !table_exists(pool, table).await? {
            return Err(AppError::Validation(format!(
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
                return Err(AppError::Validation(format!(
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
    .map_err(AppError::external)?;
    Ok(())
}

const KNOWN_MODIFIED_QUESTION_CONTRACT_CHECKSUM: &str =
    "E9A07424F1C86E99CED78966A8CB750B73D6C00E089576DD5C077B357B259BE2C34C14F0BEC320A2FB991B98B6BB9D38";

async fn column_exists(pool: &SqlitePool, table: &str, column: &str) -> AppResult<bool> {
    let statement = format!("PRAGMA table_info({table})");
    let rows = sqlx::query(AssertSqlSafe(statement))
        .fetch_all(pool)
        .await
        .map_err(AppError::external)?;
    rows.iter()
        .map(|row| row.try_get::<String, _>("name"))
        .collect::<Result<Vec<_>, _>>()
        .map(|columns| columns.iter().any(|candidate| candidate == column))
        .map_err(AppError::external)
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
