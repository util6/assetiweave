use super::*;
use crate::backend::infrastructure::conversations::test_fixtures;
use std::time::SystemTime;

fn temp_root(tag: &str) -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "assetiweave-official-{tag}-{}",
        uuid::Uuid::new_v4()
    ));
    fs::create_dir_all(&root).expect("create test root");
    root
}

fn snapshot(path: &Path) -> (Vec<u8>, Option<SystemTime>) {
    (
        fs::read(path).expect("read snapshot file"),
        fs::metadata(path)
            .expect("read snapshot metadata")
            .modified()
            .ok(),
    )
}

#[test]
fn discovery_does_not_create_a_missing_managed_directory() {
    let root = temp_root("missing");
    let managed_root = root.join("conversation-adapters");

    let adapters =
        discover_managed_conversation_adapters_at(&managed_root).expect("discover adapters");

    assert!(adapters.is_empty(), "fresh installation has no adapter");
    assert!(
        !managed_root.exists(),
        "discovery must never materialize the managed directory"
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn discovery_skips_reserved_and_unreadable_directories() {
    let root = temp_root("skipped");
    for reserved in ["packages", "runtime", "staging", "common"] {
        let dir = root.join(reserved);
        fs::create_dir_all(&dir).expect("create reserved dir");
        fs::write(dir.join(ADAPTER_MANIFEST_FILE_NAME), "{}").expect("write reserved manifest");
    }
    let broken = root.join("broken-adapter");
    fs::create_dir_all(&broken).expect("create broken adapter dir");
    fs::write(
        broken.join(ADAPTER_MANIFEST_FILE_NAME),
        "{ not a manifest }",
    )
    .expect("write unparsable manifest");

    let adapters = discover_managed_conversation_adapters_at(&root).expect("discover adapters");

    assert!(
        adapters.is_empty(),
        "reserved directories and unparsable manifests must not be projected"
    );
    assert!(
        broken.join(ADAPTER_MANIFEST_FILE_NAME).is_file(),
        "an invalid package directory is preserved, never deleted"
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn discovery_projects_existing_adapter_without_rewriting_user_edits() {
    let root = temp_root("readonly");
    let adapter_dir = root.join("codex");
    test_fixtures::copy_builtin_adapter("codex", &adapter_dir).expect("copy adapter fixture");
    let script_path = adapter_dir.join("adapter.mjs");
    let policy_path = adapter_dir.join("payload-policy.mjs");
    fs::write(&script_path, "// user revision of adapter\n").expect("edit adapter script");
    fs::write(&policy_path, "// user revision of payload policy\n").expect("edit payload policy");
    let script_before = snapshot(&script_path);
    let policy_before = snapshot(&policy_path);

    let first = discover_managed_conversation_adapters_at(&root).expect("first discovery");
    let second = discover_managed_conversation_adapters_at(&root).expect("second discovery");

    assert_eq!(first.len(), 1, "one valid adapter on disk");
    assert_eq!(second.len(), 1, "discovery is idempotent");
    assert_eq!(first[0].id, "codex");
    assert_eq!(
        first[0].trust_state,
        ConversationAdapterTrustState::BuiltIn,
        "an adapter that physically exists is projected as a managed adapter"
    );
    assert_eq!(
        snapshot(&script_path),
        script_before,
        "discovery must never rewrite user-edited adapter.mjs"
    );
    assert_eq!(
        snapshot(&policy_path),
        policy_before,
        "discovery must never rewrite user-edited payload-policy.mjs"
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn discovery_hashes_the_user_edited_script_instead_of_a_bundled_copy() {
    let root = temp_root("hash");
    let adapter_dir = root.join("codex");
    test_fixtures::copy_builtin_adapter("codex", &adapter_dir).expect("copy adapter fixture");
    let untouched =
        discover_managed_conversation_adapters_at(&root).expect("discover untouched adapter");
    fs::write(adapter_dir.join("adapter.mjs"), "// edited\n").expect("edit adapter script");

    let edited = discover_managed_conversation_adapters_at(&root).expect("discover edited adapter");

    assert_ne!(
        untouched[0].content_hash, edited[0].content_hash,
        "content hash must follow the on-disk bytes"
    );
    let _ = fs::remove_dir_all(root);
}
