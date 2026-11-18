use super::*;

fn temp_root(tag: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!(
        "assetiweave-runtime-helper-{tag}-{}",
        uuid::Uuid::new_v4()
    ));
    fs::create_dir_all(&root).expect("create runtime helper root");
    root
}

fn snapshot(path: &Path) -> (Vec<u8>, Option<std::time::SystemTime>) {
    (
        fs::read(path).expect("read snapshot file"),
        fs::metadata(path)
            .expect("read snapshot metadata")
            .modified()
            .ok(),
    )
}

#[test]
fn common_runtime_materialization_fills_missing_files_without_overwriting_existing_ones() {
    let root = temp_root("common").join(COMMON_RUNTIME_DIR_NAME);
    fs::create_dir_all(&root).expect("create common dir");
    fs::write(root.join(PAYLOAD_POLICY_FILE_NAME), "user edited policy\n")
        .expect("write user payload policy");

    materialize_common_runtime_scripts_at(&root).expect("materialize common runtime");

    assert_eq!(
        fs::read_to_string(root.join(PAYLOAD_POLICY_FILE_NAME)).expect("read user policy"),
        "user edited policy\n",
        "an existing helper file must never be overwritten"
    );
    assert_eq!(
        fs::read_to_string(root.join(SHELL_PROJECTOR_FILE_NAME)).expect("read released projector"),
        SHELL_PROJECTOR_SCRIPT,
        "missing helper files are released from the fallback copy"
    );
    assert!(
        root.join(SHELL_PROJECTOR_MANIFEST_FILE_NAME).is_file(),
        "the projector manifest is released into common/"
    );
    assert!(
        root.join(REPROJECT_RUNNER_FILE_NAME).is_file(),
        "the reproject runner is released into common/"
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn common_runtime_materialization_is_idempotent() {
    let root = temp_root("common-idempotent");
    materialize_common_runtime_scripts_at(&root).expect("materialize common runtime");
    let policy_before = snapshot(&root.join(PAYLOAD_POLICY_FILE_NAME));
    let manifest_before = snapshot(&root.join(SHELL_PROJECTOR_MANIFEST_FILE_NAME));

    materialize_common_runtime_scripts_at(&root).expect("rematerialize common runtime");

    assert_eq!(
        snapshot(&root.join(PAYLOAD_POLICY_FILE_NAME)),
        policy_before
    );
    assert_eq!(
        snapshot(&root.join(SHELL_PROJECTOR_MANIFEST_FILE_NAME)),
        manifest_before
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn shell_projector_projection_is_validated_from_the_common_manifest() {
    let root = temp_root("projector");
    materialize_common_runtime_scripts_at(&root).expect("materialize common runtime");

    let adapter = validate_common_shell_projector(&root).expect("validate managed projector");

    assert_eq!(adapter.id, "assetiweave-shell-command-projector");
    assert!(adapter
        .capabilities
        .iter()
        .any(|capability| capability == "project_command_parts"));
    assert_eq!(
        Path::new(adapter.manifest_path.as_deref().expect("manifest path")),
        root.join(SHELL_PROJECTOR_MANIFEST_FILE_NAME)
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn write_if_missing_never_touches_an_existing_file() {
    let root = temp_root("write-if-missing");
    let path = root.join("runner.mjs");

    assert!(
        write_if_missing(&path, b"fallback revision\n").expect("release fallback"),
        "a missing helper file is written"
    );
    fs::write(&path, b"user revision\n").expect("user edit");

    assert!(
        !write_if_missing(&path, b"fallback revision\n").expect("skip existing helper"),
        "an existing helper file is never rewritten"
    );
    assert_eq!(
        fs::read_to_string(&path).expect("read helper file"),
        "user revision\n"
    );
    let _ = fs::remove_dir_all(root);
}
