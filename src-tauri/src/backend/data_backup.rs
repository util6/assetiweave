use crate::backend::{
    app_settings::read_app_settings_value,
    dto::AppResult,
    path_utils::{default_database_backup_root, expand_path},
};
use chrono::Utc;
use serde::Serialize;
use serde_json::Value;
use sqlx::{
    sqlite::{SqliteConnectOptions, SqlitePoolOptions},
    SqlitePool,
};
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    time::Duration,
};
use tokio::runtime::Runtime;
use uuid::Uuid;

const DATA_BACKUP_SETTINGS_KEY: &str = "dataBackup";
const CUSTOM_DIRECTORY_KEY: &str = "customDirectory";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct DatabaseBackupReport {
    pub(crate) database_path: String,
    pub(crate) targets: Vec<DatabaseBackupTarget>,
    pub(crate) errors: Vec<DatabaseBackupError>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct DatabaseBackupTarget {
    pub(crate) directory: String,
    pub(crate) backup_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct DatabaseBackupError {
    pub(crate) directory: String,
    pub(crate) message: String,
}

pub(crate) fn backup_database_from_settings(db_path: &Path) -> AppResult<DatabaseBackupReport> {
    let mut settings_errors = Vec::new();
    let settings = match read_app_settings_value() {
        Ok(value) => value,
        Err(error) => {
            settings_errors.push(DatabaseBackupError {
                directory: "settings".to_string(),
                message: error,
            });
            Value::Object(Default::default())
        }
    };

    let default_root = default_database_backup_root()?;
    let directories = configured_backup_directories(default_root, &settings)?;
    let mut report = backup_database_to_directories(db_path, &directories)?;
    report.errors.splice(0..0, settings_errors);
    Ok(report)
}

pub(crate) fn configured_backup_directories(
    default_root: PathBuf,
    settings: &Value,
) -> AppResult<Vec<PathBuf>> {
    let mut seen = BTreeSet::new();
    let mut directories = Vec::new();
    push_unique_path(&mut directories, &mut seen, default_root);

    if let Some(custom_directory) = custom_backup_directory(settings) {
        push_unique_path(&mut directories, &mut seen, expand_path(custom_directory)?);
    }

    Ok(directories)
}

pub(crate) fn backup_database_to_directories(
    db_path: &Path,
    directories: &[PathBuf],
) -> AppResult<DatabaseBackupReport> {
    if !db_path.is_file() {
        return Err(format!(
            "database file does not exist: {}",
            db_path.display()
        ));
    }

    let file_name = backup_file_name();
    let mut targets = Vec::new();
    let mut errors = Vec::new();

    for directory in directories {
        match backup_database_to_directory(db_path, directory, &file_name) {
            Ok(target) => targets.push(target),
            Err(message) => errors.push(DatabaseBackupError {
                directory: directory.to_string_lossy().to_string(),
                message,
            }),
        }
    }

    if targets.is_empty() {
        return Err(if errors.is_empty() {
            "no database backup target directories configured".to_string()
        } else {
            errors
                .iter()
                .map(|error| format!("{}: {}", error.directory, error.message))
                .collect::<Vec<_>>()
                .join("; ")
        });
    }

    Ok(DatabaseBackupReport {
        database_path: db_path.to_string_lossy().to_string(),
        targets,
        errors,
    })
}

fn backup_database_to_directory(
    db_path: &Path,
    directory: &Path,
    file_name: &str,
) -> AppResult<DatabaseBackupTarget> {
    ensure_backup_directory(directory)?;
    let target_path = directory.join(file_name);
    snapshot_sqlite_database(db_path, &target_path)?;
    Ok(DatabaseBackupTarget {
        directory: directory.to_string_lossy().to_string(),
        backup_path: target_path.to_string_lossy().to_string(),
    })
}

fn ensure_backup_directory(directory: &Path) -> AppResult<()> {
    if directory.exists() && !directory.is_dir() {
        return Err(format!(
            "database backup target is not a directory: {}",
            directory.display()
        ));
    }
    fs::create_dir_all(directory).map_err(|error| error.to_string())
}

fn snapshot_sqlite_database(db_path: &Path, target_path: &Path) -> AppResult<()> {
    let temp_path = temporary_target_path(target_path);
    if temp_path.exists() {
        fs::remove_file(&temp_path).map_err(|error| error.to_string())?;
    }

    let snapshot_result = vacuum_into(db_path, &temp_path).or_else(|vacuum_error| {
        fs::remove_file(&temp_path).ok();
        checkpoint_and_copy(db_path, &temp_path).map_err(|copy_error| {
            format!("SQLite snapshot failed: {vacuum_error}; fallback copy failed: {copy_error}")
        })
    });

    if let Err(error) = snapshot_result {
        fs::remove_file(&temp_path).ok();
        return Err(error);
    }

    fs::rename(&temp_path, target_path).map_err(|error| error.to_string())
}

fn vacuum_into(db_path: &Path, target_path: &Path) -> AppResult<()> {
    let target = target_path.to_string_lossy().to_string();
    let runtime = build_backup_runtime()?;
    runtime.block_on(async move {
        let pool = open_backup_pool(db_path).await?;
        let result = crate::backend::store::vacuum_database_into_sqlx(&pool, &target).await;
        pool.close().await;
        result
    })
}

fn checkpoint_and_copy(db_path: &Path, target_path: &Path) -> AppResult<()> {
    let runtime = build_backup_runtime()?;
    runtime.block_on(async move {
        let pool = open_backup_pool(db_path).await?;
        let result = crate::backend::store::checkpoint_database_wal_sqlx(&pool).await;
        pool.close().await;
        result
    })?;
    fs::copy(db_path, target_path)
        .map(|_| ())
        .map_err(|error| error.to_string())
}

async fn open_backup_pool(db_path: &Path) -> AppResult<SqlitePool> {
    let options = SqliteConnectOptions::new()
        .filename(db_path)
        .create_if_missing(false)
        .busy_timeout(Duration::from_secs(10));
    SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .map_err(|error| error.to_string())
}

fn build_backup_runtime() -> AppResult<Runtime> {
    tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .map_err(|error| error.to_string())
}

fn backup_file_name() -> String {
    let timestamp = Utc::now().format("%Y%m%d-%H%M%S-%3f");
    let suffix = Uuid::new_v4().to_string();
    format!("assetiweave-app-{timestamp}-{}.db", &suffix[..8])
}

fn temporary_target_path(target_path: &Path) -> PathBuf {
    target_path.with_extension("db.tmp")
}

fn custom_backup_directory(settings: &Value) -> Option<&str> {
    settings
        .get(DATA_BACKUP_SETTINGS_KEY)?
        .get(CUSTOM_DIRECTORY_KEY)?
        .as_str()
        .map(str::trim)
        .filter(|path| !path.is_empty())
}

fn push_unique_path(paths: &mut Vec<PathBuf>, seen: &mut BTreeSet<String>, path: PathBuf) {
    let key = path.to_string_lossy().to_string();
    if seen.insert(key) {
        paths.push(path);
    }
}

#[cfg(test)]
mod tests;
