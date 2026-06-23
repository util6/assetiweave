use super::*;
use rusqlite::Connection;
use serde_json::json;
use std::{fs, path::PathBuf};
use uuid::Uuid;

#[test]
fn configured_backup_directories_include_default_and_custom_paths() {
    let root = unique_temp_path("assetiweave-backup-config");
    let default_root = root.join("default");
    let custom_root = root.join("custom");
    let settings = json!({
        "dataBackup": {
            "customDirectory": custom_root.to_string_lossy()
        }
    });

    let directories = configured_backup_directories(default_root.clone(), &settings)
        .expect("configure backup directories");

    assert_eq!(directories, vec![default_root, custom_root]);
}

#[test]
fn configured_backup_directories_skip_empty_custom_path() {
    let default_root = unique_temp_path("assetiweave-backup-default");
    let settings = json!({
        "dataBackup": {
            "customDirectory": " "
        }
    });

    let directories = configured_backup_directories(default_root.clone(), &settings)
        .expect("configure backup directories");

    assert_eq!(directories, vec![default_root]);
}

#[test]
fn backup_database_to_directories_creates_readable_snapshots() {
    let root = unique_temp_path("assetiweave-backup-snapshot");
    let db_path = root.join("app.db");
    fs::create_dir_all(&root).expect("create root");
    let conn = Connection::open(&db_path).expect("open source db");
    conn.execute_batch(
        r#"
            PRAGMA journal_mode=WAL;
            CREATE TABLE notes (id INTEGER PRIMARY KEY, body TEXT NOT NULL);
            INSERT INTO notes (body) VALUES ('asset data');
            "#,
    )
    .expect("seed source db");

    let first_target = root.join("first");
    let second_target = root.join("second");
    let report =
        backup_database_to_directories(&db_path, &[first_target.clone(), second_target.clone()])
            .expect("backup database");

    assert_eq!(report.targets.len(), 2);
    assert!(report.errors.is_empty());
    for target in report.targets {
        let backup_path = PathBuf::from(target.backup_path);
        assert!(backup_path.is_file());
        assert!(backup_path.starts_with(&first_target) || backup_path.starts_with(&second_target));
        let backup_conn = Connection::open(backup_path).expect("open backup db");
        let body: String = backup_conn
            .query_row("SELECT body FROM notes WHERE id = 1", [], |row| row.get(0))
            .expect("read backup row");
        assert_eq!(body, "asset data");
    }
}

fn unique_temp_path(prefix: &str) -> PathBuf {
    std::env::temp_dir().join(format!("{prefix}-{}", Uuid::new_v4()))
}
