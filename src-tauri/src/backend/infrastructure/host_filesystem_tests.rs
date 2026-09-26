use super::*;
use std::path::PathBuf;

#[test]
fn windows_path_comparison_is_case_insensitive_and_separator_agnostic() {
    let filesystem = HostFilesystem::new(HostPlatform::Windows);

    assert!(filesystem.same_path(
        Path::new(r"C:\Users\Alice\.codex\skills"),
        Path::new("c:/users/alice/.codex/skills")
    ));
}

#[test]
fn windows_containment_rejects_prefix_collisions_and_parent_traversal() {
    let filesystem = HostFilesystem::new(HostPlatform::Windows);
    let root = Path::new(r"C:\Users\Alice\.codex\skills");

    assert!(filesystem.is_within(Path::new(r"c:\users\alice\.codex\skills\review"), root));
    assert!(!filesystem.is_within(Path::new(r"C:\Users\Alice\.codex\skills-old\review"), root));
    assert!(!filesystem.is_within(Path::new(r"C:\Users\Alice\.codex\skills\..\secrets"), root));
    assert_eq!(
        filesystem.relative_components(
            Path::new(r"c:\users\alice\.codex\skills\Review\Rules"),
            root,
        ),
        Some(vec!["review".to_string(), "rules".to_string()])
    );
}

#[test]
fn unix_path_comparison_remains_case_sensitive() {
    let filesystem = HostFilesystem::new(HostPlatform::Linux);

    assert!(!filesystem.same_path(Path::new("/home/Alice"), Path::new("/home/alice")));
}

#[test]
fn windows_directory_symlinks_use_directory_removal() {
    assert_eq!(
        symlink_removal(HostPlatform::Windows, SymlinkKind::Directory),
        SymlinkRemoval::Directory
    );
    assert_eq!(
        symlink_removal(HostPlatform::Windows, SymlinkKind::File),
        SymlinkRemoval::File
    );
    assert_eq!(
        symlink_removal(HostPlatform::Macos, SymlinkKind::Directory),
        SymlinkRemoval::File
    );
}

#[test]
fn windows_symlink_privilege_errors_explain_the_required_host_setting() {
    let message = format_symlink_error(
        HostPlatform::Windows,
        std::io::Error::from_raw_os_error(1314),
    );

    assert!(message.contains("Developer Mode"));
    assert!(message.contains("elevated permissions"));
}

#[test]
fn copy_dir_surfaces_walk_errors_instead_of_silently_succeeding() {
    let filesystem = HostFilesystem::new(HostPlatform::current());
    let root = std::env::temp_dir().join(format!(
        "assetiweave-host-filesystem-missing-{}",
        uuid::Uuid::new_v4()
    ));
    let missing = root.join("missing");
    let target = root.join("target");

    let error = filesystem
        .copy_dir(&missing, &target)
        .expect_err("missing traversal root must fail");

    assert!(error.to_string().contains("missing") || error.to_string().contains("No such file"));
    let _ = std::fs::remove_dir_all(PathBuf::from(root));
}

#[test]
fn portable_path_segments_reject_traversal_drive_paths_and_windows_reserved_names() {
    let filesystem = HostFilesystem::new(HostPlatform::Macos);

    assert_eq!(
        filesystem
            .validate_path_segment("code-review")
            .expect("valid segment"),
        "code-review"
    );
    for invalid in [
        "../escape",
        r"C:\temp",
        "skill/name",
        "CON",
        "skill.",
        "skill ",
    ] {
        assert!(
            filesystem.validate_path_segment(invalid).is_err(),
            "expected invalid path segment: {invalid}"
        );
    }
}

#[test]
fn portable_relative_paths_reject_windows_reserved_segments() {
    let filesystem = HostFilesystem::new(HostPlatform::Windows);

    assert!(filesystem
        .validate_portable_relative_path("package/CON.txt")
        .is_err());
    assert!(filesystem
        .validate_portable_relative_path("package/file.txt.")
        .is_err());
    assert!(filesystem
        .validate_portable_relative_path("package/file:stream")
        .is_err());
}

#[test]
fn portable_relative_paths_have_case_insensitive_collision_keys() {
    let filesystem = HostFilesystem::new(HostPlatform::Windows);

    let upper = filesystem
        .validate_portable_relative_path("Package/Adapter.js")
        .expect("validate upper path");
    let lower = filesystem
        .validate_portable_relative_path("package/adapter.js")
        .expect("validate lower path");

    assert_eq!(upper.comparison_key(), lower.comparison_key());
    assert_eq!(upper.as_path(), Path::new("Package").join("Adapter.js"));
}

#[cfg(windows)]
#[test]
fn windows_removes_broken_directory_symlinks_without_touching_the_parent() {
    use std::os::windows::fs::FileTypeExt;

    let filesystem = HostFilesystem::new(HostPlatform::Windows);
    let root = std::env::temp_dir().join(format!(
        "assetiweave-windows-directory-symlink-{}",
        uuid::Uuid::new_v4()
    ));
    let source = root.join("source");
    let target = root.join("target");
    fs::create_dir_all(&source).expect("create source directory");
    filesystem
        .create_symlink(&source, &target)
        .expect("create directory symlink");
    assert!(fs::symlink_metadata(&target)
        .expect("target metadata")
        .file_type()
        .is_symlink_dir());

    fs::remove_dir_all(&source).expect("remove source directory");
    filesystem
        .remove_symlink(&target)
        .expect("remove broken directory symlink");

    assert!(fs::symlink_metadata(&target).is_err());
    assert!(root.is_dir());
    fs::remove_dir_all(root).expect("remove test root");
}

#[cfg(unix)]
#[test]
fn unix_non_utf8_os_string_path_operations_and_comparisons() {
    use std::os::unix::ffi::{OsStrExt, OsStringExt};

    let filesystem = HostFilesystem::new(HostPlatform::Linux);

    // Construct non-UTF-8 paths with invalid UTF-8 bytes: 0xFF, 0xFE, 0xFD
    let base = std::ffi::OsString::from_vec(b"/home/user/assets\xFF".to_vec());
    let child1 = std::ffi::OsString::from_vec(b"/home/user/assets\xFF/sub\xFE".to_vec());
    let child2 = std::ffi::OsString::from_vec(b"/home/user/assets\xFF/sub\xFD".to_vec());

    let base_path = Path::new(&base);
    let child1_path = Path::new(&child1);
    let child2_path = Path::new(&child2);

    // Same path equality
    assert!(filesystem.same_path(base_path, base_path));
    assert!(filesystem.same_path(child1_path, child1_path));
    // Different non-UTF-8 bytes must not be considered equal (which lossy conversion would cause)
    assert!(!filesystem.same_path(child1_path, child2_path));

    // Containment check (is_within) works without lossy conversion
    assert!(filesystem.is_within(child1_path, base_path));
    assert!(filesystem.is_within(child2_path, base_path));
    assert!(!filesystem.is_within(base_path, child1_path));

    // Relative components OS
    let rel = filesystem
        .relative_components_os(child1_path, base_path)
        .expect("must find relative components");
    assert_eq!(rel.len(), 1);
    assert_eq!(rel[0].as_os_str().as_bytes(), b"sub\xFE");

    // Prove actual filesystem file creation and same_path work with non-UTF-8 OsString.
    // Note: macOS APFS kernel enforces valid UTF-8 and rejects non-UTF-8 with EILSEQ (errno 92),
    // whereas Linux ext4/tmpfs accepts arbitrary non-zero bytes.
    let temp_dir =
        std::env::temp_dir().join(format!("assetiweave-non-utf8-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&temp_dir).expect("create temp dir");
    let non_utf8_filename = std::ffi::OsString::from_vec(b"file_\xFF\xFE.txt".to_vec());
    let file_path = temp_dir.join(non_utf8_filename);
    match fs::write(&file_path, b"hello non-utf8") {
        Ok(()) => {
            let current_fs = HostFilesystem::current();
            assert!(current_fs.same_path(&file_path, &file_path));
            assert!(current_fs.is_within(&file_path, &temp_dir));
            let read_content = fs::read(&file_path).expect("read non-utf8 file");
            assert_eq!(read_content, b"hello non-utf8");
        }
        Err(err) if err.raw_os_error() == Some(92) => {
            // macOS APFS kernel rejected non-UTF-8 filename with EILSEQ as expected.
            // Verify with multi-byte non-ASCII filename on disk.
            let non_ascii_filename =
                std::ffi::OsString::from_vec("文件_测试_🦀.txt".as_bytes().to_vec());
            let non_ascii_path = temp_dir.join(non_ascii_filename);
            fs::write(&non_ascii_path, b"hello non-ascii").expect("write non-ascii file");
            let current_fs = HostFilesystem::current();
            assert!(current_fs.same_path(&non_ascii_path, &non_ascii_path));
            assert!(current_fs.is_within(&non_ascii_path, &temp_dir));
            let read_content = fs::read(&non_ascii_path).expect("read non-ascii file");
            assert_eq!(read_content, b"hello non-ascii");
        }
        Err(err) => panic!("unexpected fs::write error: {err:?}"),
    }

    let _ = fs::remove_dir_all(&temp_dir);
}
