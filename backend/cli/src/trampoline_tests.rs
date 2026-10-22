use super::*;
use std::fs;
use tempfile::tempdir;

#[test]
fn test_is_development_mode_values() {
    assert!(is_development_mode_values(Some("development"), None));
    assert!(is_development_mode_values(Some("dev"), None));
    assert!(is_development_mode_values(Some("DEVELOPMENT "), None));
    assert!(is_development_mode_values(None, Some("1")));
    assert!(is_development_mode_values(None, Some("true")));
    assert!(is_development_mode_values(None, Some("TRUE")));

    assert!(!is_development_mode_values(Some("production"), None));
    assert!(!is_development_mode_values(Some("installed"), None));
    assert!(!is_development_mode_values(None, Some("0")));
    assert!(!is_development_mode_values(None, Some("false")));
    assert!(!is_development_mode_values(None, None));
    assert!(!is_development_mode_values(Some(""), Some("")));
}

#[test]
fn test_is_assetiweave_workspace() {
    let temp = tempdir().unwrap();
    let dir = temp.path();
    assert!(!is_assetiweave_workspace(dir));

    let other_pkg = dir.join("package.json");
    fs::write(&other_pkg, r#"{"name": "some-other-pkg"}"#).unwrap();
    assert!(!is_assetiweave_workspace(dir));

    fs::write(&other_pkg, r#"{"name": "assetiweave"}"#).unwrap();
    assert!(is_assetiweave_workspace(dir));
}

#[test]
fn test_is_assetiweave_workspace_fallback() {
    let temp = tempdir().unwrap();
    let dir = temp.path();

    let backend_cli = dir.join("backend").join("cli");
    let asset_core = dir.join("backend").join("asset_core");
    fs::create_dir_all(&backend_cli).unwrap();
    fs::create_dir_all(&asset_core).unwrap();
    fs::write(backend_cli.join("Cargo.toml"), "# mock").unwrap();

    assert!(is_assetiweave_workspace(dir));
}

#[test]
fn test_find_workspace_cli_from_in_workspace() {
    let temp = tempdir().unwrap();
    let ws_dir = temp.path();

    fs::write(ws_dir.join("package.json"), r#"{"name": "assetiweave"}"#).unwrap();

    // No target/debug binary yet
    assert_eq!(find_workspace_cli_from(ws_dir), None);

    // Create target/debug/aiwc
    let debug_dir = ws_dir.join("target").join("debug");
    fs::create_dir_all(&debug_dir).unwrap();
    let aiwc_bin = debug_dir.join(executable_name("aiwc"));
    fs::write(&aiwc_bin, b"binary placeholder").unwrap();

    assert_eq!(find_workspace_cli_from(ws_dir), Some(aiwc_bin.clone()));

    // From deep sub-directory
    let nested = ws_dir.join("backend").join("cli").join("src");
    fs::create_dir_all(&nested).unwrap();
    assert_eq!(find_workspace_cli_from(&nested), Some(aiwc_bin));
}

#[test]
fn test_find_workspace_cli_from_not_in_workspace() {
    let temp = tempdir().unwrap();
    let dir = temp.path();
    assert_eq!(find_workspace_cli_from(dir), None);
}

#[test]
fn test_executable_name() {
    let name = executable_name("aiwc");
    #[cfg(windows)]
    assert!(name.ends_with(".exe"));
    #[cfg(not(windows))]
    assert_eq!(name, "aiwc");
}
