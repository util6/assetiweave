use super::*;
use std::fs;
use std::path::PathBuf;

fn unique_temp_dir(prefix: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("{prefix}_{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&dir).expect("create unique temp dir");
    dir
}

#[test]
fn noop_when_target_already_has_valid_db() {
    let tmp = unique_temp_dir("storage_mig_noop_valid");
    let target_home = tmp.join("home");
    let legacy_dir = tmp.join("legacy");
    fs::create_dir_all(&target_home).unwrap();
    fs::create_dir_all(&legacy_dir).unwrap();

    let target_db = target_home.join("app.db");
    fs::write(&target_db, b"existing-valid-data").unwrap();

    let legacy_db = legacy_dir.join("app.db");
    fs::write(&legacy_db, b"legacy-data").unwrap();

    let migrated = migrate_legacy_storage_from(&target_home, &target_db, &legacy_dir).unwrap();
    assert!(!migrated);
    assert_eq!(fs::read(&target_db).unwrap(), b"existing-valid-data");
    assert!(!target_home
        .join("library")
        .join("database-backups")
        .exists());
    let _ = fs::remove_dir_all(&tmp);
}

#[test]
fn noop_when_legacy_db_missing_or_empty() {
    let tmp = unique_temp_dir("storage_mig_noop_empty");
    let target_home = tmp.join("home");
    let legacy_dir = tmp.join("legacy");
    fs::create_dir_all(&target_home).unwrap();
    fs::create_dir_all(&legacy_dir).unwrap();

    let target_db = target_home.join("app.db");

    // Case 1: legacy db missing
    let migrated = migrate_legacy_storage_from(&target_home, &target_db, &legacy_dir).unwrap();
    assert!(!migrated);

    // Case 2: legacy db is 0 bytes
    let legacy_db = legacy_dir.join("app.db");
    fs::write(&legacy_db, b"").unwrap();
    let migrated = migrate_legacy_storage_from(&target_home, &target_db, &legacy_dir).unwrap();
    assert!(!migrated);
    let _ = fs::remove_dir_all(&tmp);
}

#[test]
fn migrates_legacy_db_and_companions_and_archives_zero_byte_placeholder() {
    let tmp = unique_temp_dir("storage_mig_full");
    let target_home = tmp.join("home");
    let legacy_dir = tmp.join("legacy");
    fs::create_dir_all(&target_home).unwrap();
    fs::create_dir_all(&legacy_dir).unwrap();

    // Target has stale 0-byte placeholders
    let target_db = target_home.join("app.db");
    fs::write(&target_db, b"").unwrap();
    let target_placeholder = target_home.join("assetiweave.db");
    fs::write(&target_placeholder, b"").unwrap();

    // Legacy has real database and companion directories
    let legacy_db = legacy_dir.join("app.db");
    fs::write(&legacy_db, b"sqlite-header-and-data-12345").unwrap();
    fs::write(legacy_dir.join("app.db-wal"), b"wal-journal-content").unwrap();
    fs::write(legacy_dir.join("app.db-shm"), b"shm-content").unwrap();

    let legacy_memory = legacy_dir.join("memory").join("global");
    fs::create_dir_all(&legacy_memory).unwrap();
    fs::write(legacy_memory.join("MEMORY.md"), b"# Global Memory Content").unwrap();

    let legacy_index = legacy_dir.join("conversation-search-index").join("shard");
    fs::create_dir_all(&legacy_index).unwrap();
    fs::write(legacy_index.join("index.bin"), b"tantivy-index-bytes").unwrap();

    // Perform migration
    let migrated = migrate_legacy_storage_from(&target_home, &target_db, &legacy_dir).unwrap();
    assert!(migrated);

    // Verify target database
    assert_eq!(
        fs::read(&target_db).unwrap(),
        b"sqlite-header-and-data-12345"
    );
    assert_eq!(
        fs::read(target_home.join("app.db-wal")).unwrap(),
        b"wal-journal-content"
    );
    assert_eq!(
        fs::read(target_home.join("app.db-shm")).unwrap(),
        b"shm-content"
    );

    // Verify companion folders moved into target_home
    assert_eq!(
        fs::read(target_home.join("memory").join("global").join("MEMORY.md")).unwrap(),
        b"# Global Memory Content"
    );
    assert_eq!(
        fs::read(
            target_home
                .join("conversation-search-index")
                .join("shard")
                .join("index.bin")
        )
        .unwrap(),
        b"tantivy-index-bytes"
    );

    // Verify backup created
    let backup_dir = target_home
        .join("library")
        .join("database-backups")
        .join("legacy-application-support-backup");
    assert_eq!(
        fs::read(backup_dir.join("app.db")).unwrap(),
        b"sqlite-header-and-data-12345"
    );
    assert_eq!(
        fs::read(backup_dir.join("app.db-wal")).unwrap(),
        b"wal-journal-content"
    );

    // Verify 0-byte placeholder archived
    let archive_dir = target_home.join("library").join("legacy-archive");
    assert!(archive_dir.join("stale-empty-assetiweave.db").exists());

    // Verify migration notice in legacy dir
    let notice = fs::read_to_string(legacy_dir.join("MIGRATED_TO_HOME.txt")).unwrap();
    assert!(notice.contains("AssetIWeave storage was migrated from this directory to"));
    let _ = fs::remove_dir_all(&tmp);
}

#[test]
fn remaps_legacy_path_strings_in_sqlite_tables() {
    let tmp = unique_temp_dir("storage_mig_remap");
    let target_home = tmp.join("home");
    let legacy_dir = tmp.join("legacy");
    fs::create_dir_all(&target_home).unwrap();
    fs::create_dir_all(&legacy_dir).unwrap();

    let target_db = target_home.join("app.db");

    let conn = rusqlite::Connection::open(&target_db).unwrap();
    conn.execute_batch(
        r#"
        CREATE TABLE project_memories (
            id TEXT PRIMARY KEY,
            document_path TEXT
        );
        CREATE TABLE global_memories (
            id TEXT PRIMARY KEY,
            summary_document_path TEXT,
            memory_document_path TEXT
        );
        CREATE TABLE agent_execution_bindings (
            id TEXT PRIMARY KEY,
            workspace_path TEXT
        );
        "#,
    )
    .unwrap();

    let legacy_doc = legacy_dir.join("memory/projects/1/MEMORY.md");
    let legacy_summary = legacy_dir.join("memory/global/1/summary.md");
    let legacy_mem = legacy_dir.join("memory/global/1/MEMORY.md");
    let legacy_ws = legacy_dir.join("agent-executions/1");

    conn.execute(
        "INSERT INTO project_memories (id, document_path) VALUES ('1', ?1)",
        rusqlite::params![legacy_doc.to_string_lossy()],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO global_memories (id, summary_document_path, memory_document_path) VALUES ('1', ?1, ?2)",
        rusqlite::params![legacy_summary.to_string_lossy(), legacy_mem.to_string_lossy()],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO agent_execution_bindings (id, workspace_path) VALUES ('1', ?1)",
        rusqlite::params![legacy_ws.to_string_lossy()],
    )
    .unwrap();
    drop(conn);

    remap_legacy_db_paths(&target_db, &legacy_dir, &target_home).unwrap();

    let conn = rusqlite::Connection::open(&target_db).unwrap();
    let remapped_doc: String = conn
        .query_row(
            "SELECT document_path FROM project_memories WHERE id = '1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let (remapped_summary, remapped_mem): (String, String) = conn
        .query_row(
            "SELECT summary_document_path, memory_document_path FROM global_memories WHERE id = '1'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    let remapped_ws: String = conn
        .query_row(
            "SELECT workspace_path FROM agent_execution_bindings WHERE id = '1'",
            [],
            |r| r.get(0),
        )
        .unwrap();

    assert_eq!(
        remapped_doc,
        target_home
            .join("memory/projects/1/MEMORY.md")
            .to_string_lossy()
    );
    assert_eq!(
        remapped_summary,
        target_home
            .join("memory/global/1/summary.md")
            .to_string_lossy()
    );
    assert_eq!(
        remapped_mem,
        target_home
            .join("memory/global/1/MEMORY.md")
            .to_string_lossy()
    );
    assert_eq!(
        remapped_ws,
        target_home.join("agent-executions/1").to_string_lossy()
    );

    let _ = fs::remove_dir_all(&tmp);
}
