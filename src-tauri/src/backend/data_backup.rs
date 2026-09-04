//! OneShot / Shutdown Infrastructure Path.
//!
//! This module intentionally owns an independent Tokio runtime and SQLite
//! pool because it may run while the resident `AppRuntime` is closing or
//! before it has been created. It is a database close/recovery exception, not
//! a pattern for ordinary Application Service work.

use crate::backend::{
    path_utils::{default_database_backup_root, expand_path},
    runtime::{AppError, AppResult},
};
use chrono::Utc;
use rusqlite::{Connection, OpenFlags};
use serde::Serialize;
use serde_json::Value;
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};
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

pub(crate) fn backup_database_from_settings_value(
    db_path: &Path,
    settings: &Value,
) -> AppResult<DatabaseBackupReport> {
    let default_root = default_database_backup_root()?;
    let directories = configured_backup_directories(default_root, settings)?;
    backup_database_to_directories(db_path, &directories)
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
        return Err(AppError::NotFound(format!(
            "database file does not exist: {}",
            db_path.display()
        )));
    }

    let file_name = backup_file_name();
    let mut targets = Vec::new();
    let mut errors = Vec::new();

    for directory in directories {
        match backup_database_to_directory(db_path, directory, &file_name) {
            Ok(target) => targets.push(target),
            Err(message) => errors.push(DatabaseBackupError {
                directory: directory.to_string_lossy().to_string(),
                message: message.to_string(),
            }),
        }
    }

    if targets.is_empty() {
        return Err(AppError::External(if errors.is_empty() {
            "no database backup target directories configured".to_string()
        } else {
            errors
                .iter()
                .map(|error| format!("{}: {}", error.directory, error.message))
                .collect::<Vec<_>>()
                .join("; ")
        }));
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
    if let Err(error) = verify_sqlite_snapshot(&target_path) {
        fs::remove_file(&target_path).ok();
        return Err(error);
    }
    Ok(DatabaseBackupTarget {
        directory: directory.to_string_lossy().to_string(),
        backup_path: target_path.to_string_lossy().to_string(),
    })
}

fn verify_sqlite_snapshot(path: &Path) -> AppResult<()> {
    let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(AppError::external)?;
    let result = connection
        .query_row("PRAGMA quick_check", [], |row| row.get::<_, String>(0))
        .map_err(AppError::external)?;
    if result == "ok" {
        return Ok(());
    }
    Err(AppError::Validation(format!(
        "database backup verification failed for {}: {result}",
        path.display()
    )))
}

fn ensure_backup_directory(directory: &Path) -> AppResult<()> {
    if directory.exists() && !directory.is_dir() {
        return Err(AppError::Conflict(format!(
            "database backup target is not a directory: {}",
            directory.display()
        )));
    }
    Ok(fs::create_dir_all(directory).map_err(AppError::external)?)
}

fn snapshot_sqlite_database(db_path: &Path, target_path: &Path) -> AppResult<()> {
    // OneShot / Shutdown Infrastructure Path: VACUUM INTO needs a short-lived
    // independent connection so the resident pool can be closing safely.
    let temp_path = temporary_target_path(target_path);
    if temp_path.exists() {
        fs::remove_file(&temp_path).map_err(AppError::external)?;
    }

    let snapshot_result = vacuum_into(db_path, &temp_path).or_else(|vacuum_error| {
        fs::remove_file(&temp_path).ok();
        checkpoint_and_copy(db_path, &temp_path).map_err(|copy_error| {
            AppError::External(format!(
                "SQLite snapshot failed: {vacuum_error}; fallback copy failed: {copy_error}"
            ))
        })
    });

    if let Err(error) = snapshot_result {
        fs::remove_file(&temp_path).ok();
        return Err(error);
    }

    Ok(fs::rename(&temp_path, target_path).map_err(AppError::external)?)
}

fn vacuum_into(db_path: &Path, target_path: &Path) -> AppResult<()> {
    let target = target_path.to_string_lossy().to_string();
    let conn = Connection::open_with_flags(
        db_path,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(AppError::external)?;
    conn.execute("VACUUM main INTO ?1", rusqlite::params![target])
        .map_err(AppError::external)?;
    Ok(())
}

fn checkpoint_and_copy(db_path: &Path, target_path: &Path) -> AppResult<()> {
    let conn = Connection::open_with_flags(
        db_path,
        OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .map_err(AppError::external)?;
    conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
        .map_err(AppError::external)?;
    drop(conn);
    fs::copy(db_path, target_path).map_err(AppError::external)?;
    Ok(())
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
