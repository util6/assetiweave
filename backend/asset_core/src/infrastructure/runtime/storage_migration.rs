use std::{
    fs, io,
    path::{Path, PathBuf},
};

use crate::backend::infrastructure::{InfraError, InfraResult};

pub fn legacy_app_data_dir() -> Option<PathBuf> {
    directories::ProjectDirs::from("", "", "AssetIWeave").map(|proj| proj.data_dir().to_path_buf())
}

/// Automatically migrates storage from the legacy OS application directory
/// (e.g., `~/Library/Application Support/AssetIWeave/`) to the converged
/// `~/.assetiweave/` user home directory if needed.
pub fn migrate_legacy_storage_if_needed(target_db_path: &Path) -> InfraResult<bool> {
    if target_db_path.exists() {
        if let Ok(metadata) = fs::metadata(target_db_path) {
            if metadata.len() > 0 {
                return Ok(false);
            }
        }
    }

    if let Ok(config) = crate::backend::infrastructure::runtime::config::runtime_config() {
        if target_db_path != config.db_path {
            return Ok(false);
        }
    }

    let Some(target_home) = target_db_path.parent() else {
        return Ok(false);
    };

    let Some(legacy_dir) = legacy_app_data_dir() else {
        return Ok(false);
    };

    if legacy_dir == target_home {
        return Ok(false);
    }

    migrate_legacy_storage_from(target_home, target_db_path, &legacy_dir)
}

pub fn migrate_legacy_storage_from(
    target_home: &Path,
    target_db_path: &Path,
    legacy_dir: &Path,
) -> InfraResult<bool> {
    if target_db_path.exists() {
        if let Ok(m) = fs::metadata(target_db_path) {
            if m.len() > 0 {
                return Ok(false);
            }
        }
    }

    let legacy_db = legacy_dir.join("app.db");
    if !legacy_db.exists() {
        return Ok(false);
    }

    let metadata = match fs::metadata(&legacy_db) {
        Ok(m) if m.len() > 0 => m,
        _ => return Ok(false),
    };

    tracing::info!(
        legacy_dir = %legacy_dir.display(),
        target_db = %target_db_path.display(),
        db_size_bytes = metadata.len(),
        "Detected legacy database; migrating storage to converged home directory"
    );

    // 1. Create a safe cold backup of the legacy database files
    let backup_dir = target_home
        .join("library")
        .join("database-backups")
        .join("legacy-application-support-backup");
    fs::create_dir_all(&backup_dir)?;
    copy_file_if_exists(&legacy_db, &backup_dir.join("app.db"))?;
    copy_file_if_exists(
        &legacy_dir.join("app.db-wal"),
        &backup_dir.join("app.db-wal"),
    )?;
    copy_file_if_exists(
        &legacy_dir.join("app.db-shm"),
        &backup_dir.join("app.db-shm"),
    )?;

    // 2. Clear or archive any 0-byte placeholder database files in target_home
    if target_db_path.exists() {
        if let Ok(m) = fs::metadata(target_db_path) {
            if m.len() == 0 {
                let _ = fs::remove_file(target_db_path);
            }
        }
    }
    let placeholder_assetiweave_db = target_home.join("assetiweave.db");
    if placeholder_assetiweave_db.exists() {
        if let Ok(m) = fs::metadata(&placeholder_assetiweave_db) {
            if m.len() == 0 {
                let archive_dir = target_home.join("library").join("legacy-archive");
                let _ = fs::create_dir_all(&archive_dir);
                let _ = fs::rename(
                    &placeholder_assetiweave_db,
                    archive_dir.join("stale-empty-assetiweave.db"),
                );
            }
        }
    }

    // 3. Move or copy the database and WAL/SHM sidecars to target_home
    move_or_copy_file(&legacy_db, target_db_path)?;
    move_file_if_exists(
        &legacy_dir.join("app.db-wal"),
        &target_home.join("app.db-wal"),
    )?;
    move_file_if_exists(
        &legacy_dir.join("app.db-shm"),
        &target_home.join("app.db-shm"),
    )?;

    // 4. Move companion directories if present in legacy directory
    for companion in [
        "conversation-search-index",
        "memory",
        "target-providers",
        "agent-executions",
    ] {
        let src_companion = legacy_dir.join(companion);
        let dst_companion = target_home.join(companion);
        if src_companion.exists() && !dst_companion.exists() {
            let _ = move_or_copy_dir_recursively(&src_companion, &dst_companion);
        }
    }

    // 5. Remap legacy path strings in database if needed
    let _ = remap_legacy_db_paths(target_db_path, legacy_dir, target_home);

    // 6. Leave migration marker in legacy directory
    let marker_path = legacy_dir.join("MIGRATED_TO_HOME.txt");
    let marker_content = format!(
        "AssetIWeave storage was migrated from this directory to {} on {}\n",
        target_home.display(),
        chrono::Utc::now().to_rfc3339()
    );
    let _ = fs::write(marker_path, marker_content);

    tracing::info!(
        target_home = %target_home.display(),
        "Legacy storage successfully migrated to converged home directory"
    );

    Ok(true)
}

fn copy_file_if_exists(src: &Path, dst: &Path) -> InfraResult<bool> {
    if src.exists() {
        if let Some(parent) = dst.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(src, dst)?;
        Ok(true)
    } else {
        Ok(false)
    }
}

fn move_file_if_exists(src: &Path, dst: &Path) -> InfraResult<bool> {
    if src.exists() {
        move_or_copy_file(src, dst)?;
        Ok(true)
    } else {
        Ok(false)
    }
}

fn move_or_copy_file(src: &Path, dst: &Path) -> InfraResult<()> {
    if let Some(parent) = dst.parent() {
        fs::create_dir_all(parent)?;
    }
    if fs::rename(src, dst).is_err() {
        fs::copy(src, dst)?;
        let _ = fs::remove_file(src);
    }
    Ok(())
}

fn move_or_copy_dir_recursively(src: &Path, dst: &Path) -> InfraResult<()> {
    if !src.exists() {
        return Ok(());
    }
    if fs::rename(src, dst).is_ok() {
        return Ok(());
    }
    fs::create_dir_all(dst)?;
    for entry in walkdir::WalkDir::new(src).min_depth(1) {
        let entry = entry.map_err(|e| InfraError::Io(io::Error::new(io::ErrorKind::Other, e)))?;
        let rel_path = entry
            .path()
            .strip_prefix(src)
            .map_err(|e| InfraError::Io(io::Error::new(io::ErrorKind::Other, e)))?;
        let target_path = dst.join(rel_path);
        if entry.file_type().is_dir() {
            fs::create_dir_all(&target_path)?;
        } else {
            fs::copy(entry.path(), &target_path)?;
        }
    }
    let _ = fs::remove_dir_all(src);
    Ok(())
}

fn remap_legacy_db_paths(
    target_db_path: &Path,
    legacy_dir: &Path,
    target_home: &Path,
) -> InfraResult<()> {
    if !target_db_path.exists() {
        return Ok(());
    }

    // Verify SQLite magic header before attempting rusqlite connection
    if let Ok(mut file) = fs::File::open(target_db_path) {
        use std::io::Read;
        let mut header = [0u8; 16];
        if file.read_exact(&mut header).is_err() || &header != b"SQLite format 3\0" {
            return Ok(());
        }
    } else {
        return Ok(());
    }

    let legacy_prefix = legacy_dir.to_string_lossy();
    let new_prefix = target_home.to_string_lossy();
    let pattern = format!("%{}%", legacy_prefix);

    let conn = match rusqlite::Connection::open(target_db_path) {
        Ok(c) => c,
        Err(_) => return Ok(()),
    };

    // 1. project_memories
    let _ = conn.execute(
        "UPDATE project_memories SET document_path = REPLACE(document_path, ?1, ?2) WHERE document_path LIKE ?3",
        rusqlite::params![legacy_prefix, new_prefix, pattern],
    );

    // 2. global_memories
    let _ = conn.execute(
        "UPDATE global_memories SET summary_document_path = REPLACE(summary_document_path, ?1, ?2), memory_document_path = REPLACE(memory_document_path, ?1, ?2) WHERE summary_document_path LIKE ?3 OR memory_document_path LIKE ?3",
        rusqlite::params![legacy_prefix, new_prefix, pattern],
    );

    // 3. agent_execution_bindings
    let _ = conn.execute(
        "UPDATE agent_execution_bindings SET workspace_path = REPLACE(workspace_path, ?1, ?2) WHERE workspace_path LIKE ?3",
        rusqlite::params![legacy_prefix, new_prefix, pattern],
    );

    Ok(())
}

#[cfg(test)]
#[path = "storage_migration_tests.rs"]
mod tests;
