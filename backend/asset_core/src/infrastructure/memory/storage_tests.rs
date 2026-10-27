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
