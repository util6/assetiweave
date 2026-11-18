use super::*;

fn temp_root(tag: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!(
        "assetiweave-runtime-helper-{tag}-{}",
        uuid::Uuid::new_v4()
    ));
    fs::create_dir_all(&root).expect("create runtime helper root");
    root
}

#[test]
fn shell_projector_fallback_fills_missing_files_without_overwriting_existing_ones() {
    let root = temp_root("projector");
    fs::write(
        root.join("projector-adapter.cjs"),
        "user edited projector\n",
    )
    .expect("write user projector adapter");

    let adapter =
        materialize_shell_command_projector(&root).expect("materialize managed projector runtime");

    assert_eq!(adapter.id, "assetiweave-shell-command-projector");
    assert!(adapter
        .capabilities
        .iter()
        .any(|capability| capability == "project_command_parts"));
    assert_eq!(
        fs::read_to_string(root.join("projector-adapter.cjs"))
            .expect("read user projector adapter"),
        "user edited projector\n",
        "an existing helper file must never be overwritten"
    );
    assert_eq!(
        fs::read_to_string(root.join("shell-projector.cjs")).expect("read released projector"),
        SHELL_PROJECTOR_SCRIPT,
        "missing helper files are released from the fallback copy"
    );
    assert!(
        root.join("conversation-adapter.json").is_file(),
        "missing manifest is released from the fallback copy"
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
