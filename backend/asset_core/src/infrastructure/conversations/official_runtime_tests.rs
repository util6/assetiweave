use super::*;

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

#[test]
fn managed_runtime_file_writes_only_when_bytes_change() {
    let root = std::env::temp_dir().join(format!(
        "assetiweave-managed-runtime-file-{}",
        uuid::Uuid::new_v4()
    ));
    fs::create_dir_all(&root).expect("create runtime root");
    let path = root.join("runner.mjs");
    write_managed_runtime_file(&path, b"first revision\n").expect("write runtime file");
    let written = fs::metadata(&path)
        .expect("read runtime metadata")
        .modified()
        .ok();

    write_managed_runtime_file(&path, b"first revision\n").expect("rewrite identical runtime file");

    assert_eq!(
        fs::metadata(&path)
            .expect("read runtime metadata")
            .modified()
            .ok(),
        written,
        "identical bytes must not touch the file"
    );
    let _ = fs::remove_dir_all(root);
}
