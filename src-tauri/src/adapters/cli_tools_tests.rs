use super::*;

#[test]
fn path_contains_entry_matches_whole_path_segment() {
    let wanted = PathBuf::from("/Users/example/.local/bin");
    let path_env = env::join_paths([
        Path::new("/usr/bin"),
        Path::new("/Users/example/.local/bin"),
        Path::new("/bin"),
    ])
    .unwrap();
    assert!(path_contains_entry(&path_env.to_string_lossy(), &wanted));
    let bad_path_env = env::join_paths([
        Path::new("/usr/bin"),
        Path::new("/Users/example/.local/bin-extra"),
        Path::new("/bin"),
    ])
    .unwrap();
    assert!(!path_contains_entry(
        &bad_path_env.to_string_lossy(),
        &wanted
    ));
}

#[test]
fn unix_shim_execs_bundled_tool_with_original_arguments() {
    let shim = unix_shim_contents(Path::new(
        "/Applications/AssetIWeave.app/Contents/Resources/cli/assetiweave-cli",
    ));
    assert!(shim.contains(
        "exec '/Applications/AssetIWeave.app/Contents/Resources/cli/assetiweave-cli' \"$@\""
    ));
}

#[test]
fn short_cli_shim_execs_the_canonical_binary() {
    assert_eq!(CLI_ALIAS, "aiwc");
    let root = env::temp_dir().join(format!(
        "assetiweave-cli-alias-test-{}",
        uuid::Uuid::new_v4()
    ));
    fs::create_dir_all(&root).expect("create install dir");
    let target = Path::new("/Applications/AssetIWeave.app/Contents/Resources/cli/assetiweave-cli");
    write_shim(&root, CLI_NAME, target).expect("write canonical shim");
    write_shim(&root, CLI_ALIAS, target).expect("write short shim");
    assert_eq!(
        fs::read_to_string(shim_path(&root, CLI_NAME)).expect("read canonical shim"),
        fs::read_to_string(shim_path(&root, CLI_ALIAS)).expect("read short shim")
    );
    let _ = fs::remove_dir_all(root);
}

#[test]
fn installed_status_requires_the_short_cli_shim() {
    let root = env::temp_dir().join(format!(
        "assetiweave-cli-shortcut-test-{}",
        uuid::Uuid::new_v4()
    ));
    fs::create_dir_all(&root).expect("create install dir");
    fs::write(shim_path(&root, CLI_NAME), "").expect("write cli shim");
    fs::write(shim_path(&root, ENGINE_NAME), "").expect("write engine shim");
    assert!(!cli_tools_installed(&root));

    fs::write(shim_path(&root, CLI_ALIAS), "").expect("write alias shim");
    assert!(cli_tools_installed(&root));

    let _ = fs::remove_dir_all(root);
}

#[test]
fn shell_single_quote_escapes_embedded_quotes() {
    assert_eq!(shell_single_quote("/tmp/it's/cli"), "'/tmp/it'\\''s/cli'");
}

#[test]
fn status_detects_tauri_resource_layout() {
    let root = env::temp_dir().join(format!(
        "assetiweave-cli-tools-test-{}",
        uuid::Uuid::new_v4()
    ));
    let tool_dir = root.join("bundled-cli").join("cli");
    fs::create_dir_all(&tool_dir).expect("create tool dir");
    fs::write(tool_dir.join(executable_name(CLI_NAME)), "").expect("write cli");
    fs::write(tool_dir.join(executable_name(ENGINE_NAME)), "").expect("write engine");

    let status = build_status(
        &root,
        Some(
            default_install_dir()
                .unwrap_or_else(|_| fallback_install_dir())
                .to_string_lossy()
                .to_string(),
        ),
    );
    assert!(status.bundled);
    assert!(status
        .bundled_cli_path
        .as_deref()
        .is_some_and(|path| path.contains("bundled-cli")));

    let _ = fs::remove_dir_all(root);
}
