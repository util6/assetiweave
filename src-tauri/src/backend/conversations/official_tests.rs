use super::*;

#[test]
fn seed_preserves_existing_editable_workspace_files() {
    let root = std::env::temp_dir().join(format!(
        "assetiweave-official-workspace-{}",
        uuid::Uuid::new_v4()
    ));
    fs::create_dir_all(&root).expect("create workspace");
    let path = root.join("adapter.mjs");
    fs::write(&path, "user revision\n").expect("write user revision");

    write_if_missing(&path, b"bundled revision\n").expect("seed file");

    assert_eq!(
        fs::read_to_string(&path).expect("read workspace file"),
        "user revision\n"
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn managed_shell_projector_runtime_is_refreshed_and_validated() {
    let root = std::env::temp_dir().join(format!(
        "assetiweave-shell-projector-runtime-{}",
        uuid::Uuid::new_v4()
    ));
    fs::create_dir_all(&root).expect("create projector runtime");
    fs::write(root.join("projector-adapter.cjs"), "stale runtime\n")
        .expect("write stale projector runtime");

    let adapter =
        materialize_shell_command_projector(&root).expect("materialize managed projector runtime");

    assert_eq!(adapter.id, "assetiweave-shell-command-projector");
    assert!(adapter
        .capabilities
        .iter()
        .any(|capability| capability == "project_command_parts"));
    assert_eq!(
        fs::read_to_string(root.join("projector-adapter.cjs"))
            .expect("read refreshed projector runtime"),
        SHELL_PROJECTOR_ADAPTER_SCRIPT
    );
    let _ = fs::remove_dir_all(root);
}
