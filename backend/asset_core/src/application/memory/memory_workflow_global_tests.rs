use super::*;
use uuid::Uuid;

#[test]
fn test_global_memory_generation_and_idempotency() {
    let tmp = std::env::temp_dir().join(format!("assetiweave-global-{}", Uuid::new_v4()));
    std::fs::create_dir_all(&tmp).expect("create tempdir");
    let storage = MemoryStorage::new_with_root(tmp.clone(), "default");

    let initial = "# Recent\n\n- Completed SQLite persistence\n";
    storage
        .write_markdown_atomic("recent/latest.md", initial)
        .expect("write recent");

    let updated = generate_global_memory_markdown(&storage).expect("generate global");
    assert!(updated.contains("长期记忆"));
    assert!(updated.contains("SQLite persistence"));

    storage
        .write_markdown_atomic("MEMORY.md", &updated)
        .expect("write memory");

    // Second run with unchanged content produces identical content
    let second = generate_global_memory_markdown(&storage).expect("generate again");
    assert_eq!(updated, second);

    std::fs::remove_dir_all(tmp).ok();
}
