use super::*;

#[test]
fn index_roots_are_isolated_by_database_and_tenant() {
    let root = std::env::temp_dir().join("assetiweave-index-root-test");
    assert_ne!(
        conversation_search_index_root(&root.join("app.db"), "default"),
        conversation_search_index_root(&root.join("other.db"), "default")
    );
    assert_ne!(
        conversation_search_index_root(&root.join("app.db"), "default"),
        conversation_search_index_root(&root.join("app.db"), "tenant-b")
    );
}

#[cfg(unix)]
#[test]
fn search_index_directories_are_private_to_the_current_user() {
    use std::os::unix::fs::PermissionsExt;
    let root =
        std::env::temp_dir().join(format!("assetiweave-index-permissions-{}", Uuid::new_v4()));
    fs::create_dir_all(&root).expect("create permissions fixture");
    set_private_directory_permissions(&root).expect("set private permissions");
    let mode = fs::metadata(&root)
        .expect("read permissions")
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(mode, 0o700);
    let _ = fs::remove_dir_all(root);
}
