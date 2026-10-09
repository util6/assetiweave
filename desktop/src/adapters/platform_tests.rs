use super::*;

#[test]
fn reveal_path_resolves_home_shorthand_before_invoking_file_manager() {
    let resolved = resolve_reveal_path("~").expect("resolve home");

    assert!(resolved.is_absolute());
    assert!(resolved.is_dir());
    assert_ne!(resolved.file_name(), Some(std::ffi::OsStr::new("~")));
}

#[test]
fn windows_opens_directories_without_select_flag() {
    let path = Path::new(r"C:\Users\95853\.codex\skills");
    let invocation = build_file_manager_invocation(path, true, FileManagerPlatform::Windows);

    assert_eq!(invocation.program, "explorer");
    assert_eq!(invocation.args, vec![path.as_os_str().to_os_string()]);
}

#[test]
fn windows_selects_files_with_single_select_argument() {
    let path = Path::new(r"C:\Users\95853\.codex\skills\README.md");
    let invocation = build_file_manager_invocation(path, false, FileManagerPlatform::Windows);

    assert_eq!(invocation.program, "explorer");
    assert_eq!(
        invocation.args,
        vec![OsString::from(format!(
            "/select,{}",
            path.to_string_lossy()
        ))]
    );
}
