use super::*;
use std::io::Write;
use uuid::Uuid;

fn temp_package_root() -> PathBuf {
    let root = std::env::temp_dir().join(format!(
        "assetiweave-conversation-package-{}",
        Uuid::new_v4()
    ));
    fs::create_dir_all(&root).expect("create package root");
    root
}

fn write_valid_package(root: &Path) {
    fs::write(
        root.join(PACKAGE_MANIFEST_FILE),
        r#"{
  "schema_version": 1,
  "package_id": "codex-session",
  "name": "Codex Session Parser",
  "version": "1.0.0",
  "min_core_version": "0.1.0",
  "record_kind": "session",
  "adapter_manifest": "conversation-adapter.json",
  "capabilities": ["read_session"],
  "runtime": { "protocol": "stdio-ndjson-v1" },
  "changelog": []
}"#,
    )
    .expect("write package manifest");
    fs::write(
        root.join("conversation-adapter.json"),
        r#"{
  "schema_version": 1,
  "id": "codex",
  "name": "Codex",
  "version": "1.0.0",
  "protocol_version": 1,
  "runtime": { "type": "node", "entry": "adapter.mjs", "version": ">=20" },
  "capabilities": ["probe", "read_session"],
  "input_kinds": ["directory"]
}"#,
    )
    .expect("write adapter manifest");
    fs::write(root.join("adapter.mjs"), "process.exit(0);\n").expect("write adapter");
}

#[test]
fn package_manifest_rejects_unsafe_adapter_manifest_paths() {
    let root = temp_package_root();
    write_valid_package(&root);
    for unsafe_path in [
        "../conversation-adapter.json",
        "/tmp/conversation-adapter.json",
        "nested/../conversation-adapter.json",
        "C:\\tmp\\conversation-adapter.json",
    ] {
        let mut text = fs::read_to_string(root.join(PACKAGE_MANIFEST_FILE)).expect("read manifest");
        text = text.replace(
            "\"adapter_manifest\": \"conversation-adapter.json\"",
            &format!("\"adapter_manifest\": \"{unsafe_path}\""),
        );
        fs::write(root.join(PACKAGE_MANIFEST_FILE), text).expect("write manifest");
        assert!(
            validate_conversation_adapter_package_dir(&root).is_err(),
            "expected unsafe path to fail: {unsafe_path}"
        );
        write_valid_package(&root);
    }
    let _ = fs::remove_dir_all(root);
}

#[test]
fn package_manifest_rejects_missing_adapter_manifest_and_newer_core() {
    let root = temp_package_root();
    write_valid_package(&root);
    fs::remove_file(root.join("conversation-adapter.json")).expect("remove adapter manifest");
    assert!(validate_conversation_adapter_package_dir(&root).is_err());

    write_valid_package(&root);
    let text = fs::read_to_string(root.join(PACKAGE_MANIFEST_FILE))
        .expect("read package manifest")
        .replace(
            "\"min_core_version\": \"0.1.0\"",
            "\"min_core_version\": \"99.0.0\"",
        );
    fs::write(root.join(PACKAGE_MANIFEST_FILE), text).expect("write package manifest");
    assert!(validate_conversation_adapter_package_dir(&root).is_err());
    let _ = fs::remove_dir_all(root);
}

#[test]
fn package_hash_changes_when_auxiliary_file_changes() {
    let root = temp_package_root();
    write_valid_package(&root);
    let first = hash_conversation_adapter_package_dir(&root).expect("hash package");

    let mut helper = fs::File::create(root.join("helper.txt")).expect("create helper");
    helper.write_all(b"first").expect("write helper");
    let second = hash_conversation_adapter_package_dir(&root).expect("hash changed package");
    assert_ne!(first, second);

    fs::write(root.join("helper.txt"), "second").expect("mutate helper");
    let third = hash_conversation_adapter_package_dir(&root).expect("hash mutated package");
    assert_ne!(second, third);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn publisher_scoped_package_ids_are_safe_path_segments() {
    assert!(validate_safe_id("package id", "com.util6.codex-session").is_ok());
    assert!(validate_safe_id("package id", "../external").is_err());
    assert!(validate_safe_id("package id", "publisher/package").is_err());
}

#[test]
fn kernel_package_inspection_preserves_fixture_package_identity() {
    use crate::backend::extension_kernel::DomainPackageSystem;

    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../builtin-assets/adapters/codex");
    let inspected = ConversationAdapterPackageSystem
        .inspect(&root)
        .expect("inspect fixed conversation adapter package");
    assert_eq!(
        ConversationAdapterPackageSystem.kind(),
        crate::backend::extension_kernel::PackageKind::ConversationAdapter
    );
    assert_eq!(
        inspected.identity.kind,
        crate::backend::extension_kernel::PackageKind::ConversationAdapter
    );
    assert_eq!(
        inspected.identity.package_id,
        "io.github.util6.codex-session"
    );
    assert_eq!(
        inspected.identity.version,
        semver::Version::parse("1.6.4").unwrap()
    );
    assert_eq!(inspected.compatibility.protocol_version, 1);
    assert_eq!(
        inspected.invocation.kind,
        crate::backend::extension_kernel::RuntimeProgramKind::Node
    );
    assert_eq!(inspected.invocation.entry, "adapter.mjs");
    assert_eq!(inspected.invocation.version_req.as_deref(), Some(">=20"));
    assert_eq!(
        inspected.availability_probe.kind,
        crate::backend::extension_kernel::ProbeKind::Availability
    );
    assert_eq!(inspected.availability_probe.args, vec!["--version"]);
    assert!(inspected.model_discovery_probe.is_none());
}
