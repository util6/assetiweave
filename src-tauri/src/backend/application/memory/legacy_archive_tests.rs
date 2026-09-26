use super::*;
use std::sync::{Mutex, OnceLock};

fn archive_test_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

#[test]
fn archives_legacy_rows_once_without_mutating_database() {
    let _guard = archive_test_lock().lock().expect("archive test lock");
    let root = std::env::temp_dir().join(format!(
        "assetiweave-memory-archive-{}",
        uuid::Uuid::new_v4()
    ));
    let db_path = root.join("legacy.db");
    fs::create_dir_all(&root).expect("create test root");
    let connection = Connection::open(&db_path).expect("create legacy database");
    connection
        .execute_batch(
            "CREATE TABLE memory_items (id TEXT, content TEXT); \
             INSERT INTO memory_items VALUES ('current-item', 'current memory'); \
             CREATE TABLE legacy_memory_items (id TEXT, content TEXT); \
             INSERT INTO legacy_memory_items VALUES ('item-1', 'legacy note');",
        )
        .expect("insert legacy row");
    drop(connection);
    let previous_home = std::env::var_os("ASSETIWEAVE_HOME");
    std::env::set_var("ASSETIWEAVE_HOME", &root);

    let archive = archive_legacy_memory_once(&db_path)
        .expect("archive legacy memory")
        .expect("archive path");
    let first = fs::read_to_string(&archive).expect("read archive");
    assert!(first.contains("legacy note"));
    assert!(!first.contains("current memory"));
    assert!(archive_legacy_memory_once(&db_path)
        .expect("repeat archive")
        .is_some());
    assert_eq!(
        first,
        fs::read_to_string(archive).expect("read stable archive")
    );

    match previous_home {
        Some(value) => std::env::set_var("ASSETIWEAVE_HOME", value),
        None => std::env::remove_var("ASSETIWEAVE_HOME"),
    }
    fs::remove_dir_all(root).expect("remove test root");
}
