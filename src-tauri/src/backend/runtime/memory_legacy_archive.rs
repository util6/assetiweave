use crate::backend::{
    path_utils,
    runtime::{AppError, AppResult},
};
use rusqlite::{types::ValueRef, Connection};
use serde::Serialize;
use serde_json::{Map, Value};
use std::{collections::BTreeMap, fs, path::Path};

const ARCHIVE_FILE_NAME: &str = "memory-legacy.json";
const LEGACY_TABLES: &[&str] = &[
    "memory_runs",
    "memory_dream_states",
    "memory_dream_notes",
    "memory_extractions",
    "memory_items",
    "memory_item_revisions",
    "memory_evidence_snapshots",
    "memory_run_evidence",
    "memory_dream_note_evidence",
    "memory_extraction_evidence",
    "memory_item_evidence",
];

#[derive(Debug, Serialize)]
struct LegacyMemoryArchive {
    format: &'static str,
    tables: BTreeMap<String, Vec<Map<String, Value>>>,
}

/// Export legacy Memory rows once to an app-owned, human-readable file.
///
/// The reader never writes to SQLite and the resulting file is deliberately
/// outside all source directories. New Memory workflows do not consume it.
pub(crate) fn archive_legacy_memory_once(db_path: &Path) -> AppResult<Option<std::path::PathBuf>> {
    let root = path_utils::memory_legacy_archive_root()?;
    let archive_path = root.join(ARCHIVE_FILE_NAME);
    if archive_path.exists() {
        return Ok(Some(archive_path));
    }

    let connection =
        Connection::open_with_flags(db_path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(AppError::external)?;
    let mut tables = BTreeMap::new();
    for table in LEGACY_TABLES {
        if !table_exists(&connection, table)? {
            continue;
        }
        let rows = read_table(&connection, table)?;
        if !rows.is_empty() {
            tables.insert((*table).to_string(), rows);
        }
    }
    if tables.is_empty() {
        return Ok(None);
    }

    fs::create_dir_all(&root).map_err(AppError::external)?;
    let payload = serde_json::to_vec_pretty(&LegacyMemoryArchive {
        format: "assetiweave.memory-legacy.v1",
        tables,
    })
    .map_err(AppError::external)?;
    let temporary = root.join(format!(".{ARCHIVE_FILE_NAME}.tmp"));
    fs::write(&temporary, payload).map_err(AppError::external)?;
    fs::rename(&temporary, &archive_path).map_err(AppError::external)?;
    Ok(Some(archive_path))
}

fn table_exists(connection: &Connection, table: &str) -> AppResult<bool> {
    connection
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1)",
            [table],
            |row| row.get::<_, i64>(0),
        )
        .map(|value| value == 1)
        .map_err(AppError::external)
}

fn read_table(connection: &Connection, table: &str) -> AppResult<Vec<Map<String, Value>>> {
    let mut statement = connection
        .prepare(&format!("SELECT * FROM {table}"))
        .map_err(AppError::external)?;
    let columns = statement
        .column_names()
        .into_iter()
        .map(str::to_string)
        .collect::<Vec<_>>();
    let mut rows = statement.query([]).map_err(AppError::external)?;
    let mut result = Vec::new();
    while let Some(row) = rows.next().map_err(AppError::external)? {
        let mut object = Map::new();
        for (index, column) in columns.iter().enumerate() {
            let value = row.get_ref(index).map_err(AppError::external)?;
            object.insert(column.clone(), sqlite_value(value));
        }
        result.push(object);
    }
    Ok(result)
}

fn sqlite_value(value: ValueRef<'_>) -> Value {
    match value {
        ValueRef::Null => Value::Null,
        ValueRef::Integer(value) => Value::from(value),
        ValueRef::Real(value) => Value::from(value),
        ValueRef::Text(value) => Value::String(String::from_utf8_lossy(value).into_owned()),
        ValueRef::Blob(value) => Value::String(format!("hex:{}", encode_hex(value))),
    }
}

fn encode_hex(value: &[u8]) -> String {
    value.iter().map(|byte| format!("{byte:02x}")).collect()
}

#[cfg(test)]
mod tests {
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
            .execute_batch("CREATE TABLE memory_items (id TEXT, content TEXT); INSERT INTO memory_items VALUES ('item-1', 'legacy note');")
            .expect("insert legacy row");
        drop(connection);
        let previous_home = std::env::var_os("ASSETIWEAVE_HOME");
        std::env::set_var("ASSETIWEAVE_HOME", &root);

        let archive = archive_legacy_memory_once(&db_path)
            .expect("archive legacy memory")
            .expect("archive path");
        let first = fs::read_to_string(&archive).expect("read archive");
        assert!(first.contains("legacy note"));
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
}
