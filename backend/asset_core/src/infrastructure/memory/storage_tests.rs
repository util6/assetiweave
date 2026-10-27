use super::*;
use uuid::Uuid;

#[test]
fn test_memory_storage_paths_and_atomic_write_with_redaction() {
    let tmp = std::env::temp_dir().join(format!("assetiweave-storage-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&tmp).expect("create tempdir");
    let storage = MemoryStorage::new_with_root(tmp.clone(), "default");

    let sessions_dir = storage.sessions_dir();
    let recent_dir = storage.recent_dir();
    let projects_dir = storage.projects_dir();
    let global_file = storage.global_file();

    assert!(sessions_dir.ends_with("sessions"));
    assert!(recent_dir.ends_with("recent"));
    assert!(projects_dir.ends_with("projects"));
    assert!(global_file.ends_with("MEMORY.md"));

    // Write a session markdown with sensitive API token
    let raw_content = "# Session Title\n\nSecret: Bearer abcdefghijklmnopqrstuvwxyz1234567890\n";
    let rel_path = "sessions/2026-10-21_session-001.md";
    let written_path = storage
        .write_markdown_atomic(rel_path, raw_content)
        .expect("write atomic");

    assert!(written_path.exists());
    let read_back = storage
        .read_markdown(rel_path)
        .expect("read markdown")
        .expect("content exists");

    // Must be redacted
    assert!(!read_back.contains("abcdefghijklmnopqrstuvwxyz1234567890"));
    assert!(read_back.contains("[REDACTED"));

    // Reading non-existent file returns None
    let missing = storage
        .read_markdown("sessions/non_existent.md")
        .expect("read missing");
    assert!(missing.is_none());

    std::fs::remove_dir_all(tmp).ok();
}

#[test]
fn test_memory_storage_clear_all_and_reset() {
    let tmp = std::env::temp_dir().join(format!("assetiweave-clear-test-{}", Uuid::new_v4()));
    let storage = MemoryStorage::new_with_root(tmp.clone(), "default");

    // Create junk files and directory hierarchy
    storage
        .write_markdown_atomic("sessions/session-001.md", "# Session 1")
        .expect("write session");
    storage
        .write_markdown_atomic("projects/project-a.md", "# Project A")
        .expect("write project");
    storage
        .write_markdown_atomic("recent/latest.md", "# Recent snapshot")
        .expect("write recent");
    storage
        .write_markdown_atomic("MEMORY.md", "# Global memory")
        .expect("write global");
    std::fs::write(storage.root_dir().join("memory_summary.md"), "summary").expect("write summary");
    std::fs::write(storage.root_dir().join("extra_junk.tmp"), "junk").expect("write junk");

    assert!(storage.root_dir().join("sessions/session-001.md").exists());
    assert!(storage.root_dir().join("projects/project-a.md").exists());
    assert!(storage.root_dir().join("recent/latest.md").exists());
    assert!(storage.global_file().exists());
    assert!(storage.root_dir().join("memory_summary.md").exists());
    assert!(storage.root_dir().join("extra_junk.tmp").exists());

    // Execute clear_all
    storage.clear_all().expect("clear all");

    // Root dir and empty base subdirectories must exist
    assert!(storage.root_dir().exists());
    assert!(storage.sessions_dir().exists());
    assert!(storage.recent_dir().exists());
    assert!(storage.projects_dir().exists());

    // All previous files must be gone
    assert!(!storage.global_file().exists());
    assert!(!storage.root_dir().join("memory_summary.md").exists());
    assert!(!storage.root_dir().join("extra_junk.tmp").exists());

    // Subdirectories must be empty
    assert_eq!(
        std::fs::read_dir(storage.sessions_dir()).unwrap().count(),
        0
    );
    assert_eq!(
        std::fs::read_dir(storage.projects_dir()).unwrap().count(),
        0
    );
    assert_eq!(std::fs::read_dir(storage.recent_dir()).unwrap().count(), 0);

    // reset() also works idempotently on clean structure
    storage.reset().expect("reset");
    assert!(storage.sessions_dir().exists());
    assert!(storage.recent_dir().exists());
    assert!(storage.projects_dir().exists());

    std::fs::remove_dir_all(tmp).ok();
}
