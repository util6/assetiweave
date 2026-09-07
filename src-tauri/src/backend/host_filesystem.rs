use crate::backend::{
    host_paths::HostPlatform,
    runtime::{AppError, AppResult},
};
use std::{
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
};
use walkdir::WalkDir;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SymlinkKind {
    File,
    Directory,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SymlinkRemoval {
    File,
    Directory,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct HostFilesystem {
    platform: HostPlatform,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PortableRelativePath {
    path: PathBuf,
    comparison_key: String,
}

impl PortableRelativePath {
    pub(crate) fn as_path(&self) -> &Path {
        &self.path
    }

    pub(crate) fn comparison_key(&self) -> &str {
        &self.comparison_key
    }
}

impl HostFilesystem {
    pub(crate) fn current() -> Self {
        Self::new(HostPlatform::current())
    }

    pub(crate) fn new(platform: HostPlatform) -> Self {
        Self { platform }
    }

    pub(crate) fn same_path(&self, left: &Path, right: &Path) -> bool {
        self.normalized_path(left) == self.normalized_path(right)
    }

    pub(crate) fn is_within(&self, path: &Path, root: &Path) -> bool {
        self.relative_components_os(path, root).is_some()
    }

    pub(crate) fn relative_components_os(&self, path: &Path, root: &Path) -> Option<Vec<OsString>> {
        let path = self.normalized_path(path);
        let root = self.normalized_path(root);
        (path.prefix == root.prefix
            && path.absolute == root.absolute
            && path.components.starts_with(&root.components))
        .then(|| path.components[root.components.len()..].to_vec())
    }

    pub(crate) fn relative_components(&self, path: &Path, root: &Path) -> Option<Vec<String>> {
        let components = self.relative_components_os(path, root)?;
        let mut strings = Vec::with_capacity(components.len());
        for component in components {
            strings.push(component.into_string().ok()?);
        }
        Some(strings)
    }

    pub(crate) fn validate_path_segment(&self, segment: &str) -> AppResult<String> {
        if segment.ends_with([' ', '.']) {
            return Err(AppError::Validation(format!(
                "path segment must not end with a space or period: {segment}"
            )));
        }
        let segment = segment.trim();
        if segment.is_empty() || matches!(segment, "." | "..") {
            return Err(AppError::Validation(
                "path segment must not be empty, '.' or '..'".to_string(),
            ));
        }
        if segment
            .chars()
            .any(|character| character.is_control() || r#"<>:"/\|?*"#.contains(character))
        {
            return Err(AppError::Validation(format!(
                "path segment contains a platform-reserved character: {segment}"
            )));
        }
        let reserved_stem = segment
            .split('.')
            .next()
            .unwrap_or(segment)
            .to_ascii_uppercase();
        let is_reserved = matches!(
            reserved_stem.as_str(),
            "CON" | "PRN" | "AUX" | "NUL" | "CLOCK$"
        ) || reserved_stem
            .strip_prefix("COM")
            .is_some_and(is_windows_reserved_device_number)
            || reserved_stem
                .strip_prefix("LPT")
                .is_some_and(is_windows_reserved_device_number);
        if is_reserved {
            return Err(AppError::Validation(format!(
                "path segment is reserved on Windows: {segment}"
            )));
        }
        Ok(segment.to_string())
    }

    pub(crate) fn validate_portable_relative_path(
        &self,
        raw: &str,
    ) -> AppResult<PortableRelativePath> {
        let normalized = raw.replace('\\', "/");
        let normalized = normalized.trim_end_matches('/');
        let bytes = normalized.as_bytes();
        if normalized.is_empty()
            || normalized.starts_with('/')
            || normalized.starts_with("//")
            || (bytes.len() >= 2 && bytes[1] == b':' && bytes[0].is_ascii_alphabetic())
        {
            return Err(AppError::Validation(format!(
                "path must be portable and relative: {raw}"
            )));
        }

        let mut path = PathBuf::new();
        let mut comparison_components = Vec::new();
        for component in normalized.split('/') {
            let component = self.validate_path_segment(component)?;
            path.push(&component);
            comparison_components.push(if self.platform == HostPlatform::Windows {
                component.to_lowercase()
            } else {
                component
            });
        }

        Ok(PortableRelativePath {
            path,
            comparison_key: comparison_components.join("/"),
        })
    }

    pub(crate) fn create_symlink(&self, source: &Path, target: &Path) -> AppResult<()> {
        let kind = if fs::metadata(source)?.is_dir() {
            SymlinkKind::Directory
        } else {
            SymlinkKind::File
        };
        self.create_symlink_with_kind(source, target, kind)
    }

    pub(crate) fn create_symlink_with_kind(
        &self,
        source: &Path,
        target: &Path,
        kind: SymlinkKind,
    ) -> AppResult<()> {
        create_symlink_with_kind(source, target, kind)
    }

    pub(crate) fn symlink_kind(&self, path: &Path) -> AppResult<SymlinkKind> {
        let metadata = fs::symlink_metadata(path)?;
        symlink_kind(path, &metadata)
    }

    pub(crate) fn remove_symlink(&self, path: &Path) -> AppResult<()> {
        let kind = self.symlink_kind(path)?;
        match symlink_removal(self.platform, kind) {
            SymlinkRemoval::File => fs::remove_file(path),
            SymlinkRemoval::Directory => fs::remove_dir(path),
        }
        .map_err(AppError::Io)
    }

    pub(crate) fn remove_path(&self, path: &Path) -> AppResult<()> {
        let metadata = fs::symlink_metadata(path)?;
        if metadata.file_type().is_symlink() {
            return self.remove_symlink(path);
        }
        if metadata.is_file() {
            return fs::remove_file(path).map_err(AppError::Io);
        }
        if metadata.is_dir() {
            return fs::remove_dir_all(path).map_err(AppError::Io);
        }
        Err(AppError::Conflict(format!(
            "unsupported filesystem entry: {}",
            path.display()
        )))
    }

    pub(crate) fn copy_dir(&self, source: &Path, target: &Path) -> AppResult<()> {
        for entry in WalkDir::new(source) {
            let entry = entry.map_err(|error| AppError::External(error.to_string()))?;
            let relative = entry
                .path()
                .strip_prefix(source)
                .map_err(|error| AppError::External(error.to_string()))?;
            let destination = target.join(relative);
            if entry.file_type().is_dir() {
                fs::create_dir_all(&destination)?;
            } else if entry.file_type().is_file() {
                if let Some(parent) = destination.parent() {
                    fs::create_dir_all(parent)?;
                }
                fs::copy(entry.path(), destination)?;
            }
        }
        Ok(())
    }

    pub(crate) fn copy_dir_without_conflicts(&self, source: &Path, target: &Path) -> AppResult<()> {
        if !source.exists() {
            return Ok(());
        }
        if !source.is_dir() {
            return Err(AppError::Validation(format!(
                "backup source is not a directory: {}",
                source.display()
            )));
        }

        for entry in WalkDir::new(source) {
            let entry = entry.map_err(|error| AppError::External(error.to_string()))?;
            let relative = entry
                .path()
                .strip_prefix(source)
                .map_err(|error| AppError::External(error.to_string()))?;
            let destination = target.join(relative);
            if entry.file_type().is_dir() {
                fs::create_dir_all(&destination)?;
                continue;
            }
            if !entry.file_type().is_file() {
                continue;
            }

            if destination.exists() {
                if !destination.is_file() {
                    return Err(AppError::Conflict(format!(
                        "backup migration target is not a file: {}",
                        destination.display()
                    )));
                }
                let source_bytes = fs::read(entry.path())?;
                let destination_bytes = fs::read(&destination)?;
                if source_bytes != destination_bytes {
                    return Err(AppError::Conflict(format!(
                        "backup migration target already has different content: {}",
                        destination.display()
                    )));
                }
                continue;
            }

            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::copy(entry.path(), destination)?;
        }
        Ok(())
    }

    fn normalized_path(&self, path: &Path) -> NormalizedPath {
        let path = if self.platform == HostPlatform::current() {
            canonicalize_with_missing_tail(path)
        } else {
            path.to_path_buf()
        };
        NormalizedPath::from_path(self.platform, &path)
    }
}

fn is_windows_reserved_device_number(value: &str) -> bool {
    matches!(value, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9")
}

fn symlink_removal(platform: HostPlatform, kind: SymlinkKind) -> SymlinkRemoval {
    if platform == HostPlatform::Windows && kind == SymlinkKind::Directory {
        SymlinkRemoval::Directory
    } else {
        SymlinkRemoval::File
    }
}

#[cfg(unix)]
fn create_symlink_with_kind(source: &Path, target: &Path, _kind: SymlinkKind) -> AppResult<()> {
    std::os::unix::fs::symlink(source, target).map_err(AppError::Io)
}

#[cfg(windows)]
fn create_symlink_with_kind(source: &Path, target: &Path, kind: SymlinkKind) -> AppResult<()> {
    match kind {
        SymlinkKind::Directory => std::os::windows::fs::symlink_dir(source, target),
        SymlinkKind::File => std::os::windows::fs::symlink_file(source, target),
    }
    .map_err(|error| AppError::External(format_symlink_error(HostPlatform::Windows, error)))
}

#[cfg(any(windows, test))]
fn format_symlink_error(platform: HostPlatform, error: std::io::Error) -> String {
    if platform == HostPlatform::Windows && error.raw_os_error() == Some(1314) {
        return format!(
            "Windows symlink creation requires Developer Mode or elevated permissions: {error}"
        );
    }
    error.to_string()
}

#[cfg(unix)]
fn symlink_kind(path: &Path, metadata: &fs::Metadata) -> AppResult<SymlinkKind> {
    if !metadata.file_type().is_symlink() {
        return Err(AppError::Conflict(format!(
            "target is not a symlink: {}",
            path.display()
        )));
    }
    Ok(
        if fs::metadata(path)
            .map(|metadata| metadata.is_dir())
            .unwrap_or(false)
        {
            SymlinkKind::Directory
        } else {
            SymlinkKind::File
        },
    )
}

#[cfg(windows)]
fn symlink_kind(path: &Path, metadata: &fs::Metadata) -> AppResult<SymlinkKind> {
    use std::os::windows::fs::FileTypeExt;

    let file_type = metadata.file_type();
    if file_type.is_symlink_dir() {
        Ok(SymlinkKind::Directory)
    } else if file_type.is_symlink_file() {
        Ok(SymlinkKind::File)
    } else {
        Err(AppError::Conflict(format!(
            "target is not a symlink: {}",
            path.display()
        )))
    }
}

fn canonicalize_with_missing_tail(path: &Path) -> PathBuf {
    let mut candidate = path;
    let mut missing = Vec::<OsString>::new();
    loop {
        if let Ok(canonical) = candidate.canonicalize() {
            return missing
                .into_iter()
                .rev()
                .fold(canonical, |path, component| path.join(component));
        }
        let Some(name) = candidate.file_name() else {
            return path.to_path_buf();
        };
        missing.push(name.to_os_string());
        let Some(parent) = candidate.parent() else {
            return path.to_path_buf();
        };
        candidate = parent;
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct NormalizedPath {
    prefix: OsString,
    absolute: bool,
    components: Vec<OsString>,
}

impl NormalizedPath {
    fn from_path(platform: HostPlatform, path: &Path) -> Self {
        if platform != HostPlatform::Windows {
            return parse_unix_path(path);
        }
        parse_windows_path(path)
    }
}

#[cfg(not(unix))]
fn parse_unix_path(path: &Path) -> NormalizedPath {
    let raw = path.to_string_lossy();
    let value = raw.as_ref();
    let (prefix, absolute, remainder) = if value.starts_with("//") && !value.starts_with("///") {
        (OsString::from("//"), true, &value[2..])
    } else if let Some(stripped) = value.strip_prefix('/') {
        (OsString::from("/"), true, stripped)
    } else {
        (OsString::new(), false, value)
    };

    let mut components = Vec::<OsString>::new();
    for part in remainder.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                if components.last().is_some_and(|last| last != "..") {
                    components.pop();
                } else if !absolute {
                    components.push(OsString::from(".."));
                }
            }
            _ => {
                components.push(OsString::from(part));
            }
        }
    }

    NormalizedPath {
        prefix,
        absolute,
        components,
    }
}

#[cfg(unix)]
fn parse_unix_path(path: &Path) -> NormalizedPath {
    use std::os::unix::ffi::{OsStrExt, OsStringExt};

    let bytes = path.as_os_str().as_bytes();
    let (prefix, absolute, remainder) = if bytes.starts_with(b"//") && !bytes.starts_with(b"///") {
        (OsString::from("//"), true, &bytes[2..])
    } else if let Some(stripped) = bytes.strip_prefix(b"/") {
        (OsString::from("/"), true, stripped)
    } else {
        (OsString::new(), false, bytes)
    };

    let mut components = Vec::<OsString>::new();
    for part in remainder.split(|&b| b == b'/') {
        match part {
            b"" | b"." => {}
            b".." => {
                if components.last().is_some_and(|last| last != "..") {
                    components.pop();
                } else if !absolute {
                    components.push(OsString::from(".."));
                }
            }
            _ => {
                components.push(OsStringExt::from_vec(part.to_vec()));
            }
        }
    }

    NormalizedPath {
        prefix,
        absolute,
        components,
    }
}

fn parse_windows_path(path: &Path) -> NormalizedPath {
    if let Some(raw) = path.to_str() {
        let mut value = raw.replace('\\', "/");
        value.make_ascii_lowercase();

        let (prefix, absolute, remainder) = if let Some(remainder) = value.strip_prefix("//") {
            ("//".to_string(), true, remainder)
        } else if value.as_bytes().get(1) == Some(&b':') {
            let prefix = value[..2].to_string();
            let remainder = &value[2..];
            (prefix, remainder.starts_with('/'), remainder)
        } else if let Some(remainder) = value.strip_prefix('/') {
            ("/".to_string(), true, remainder)
        } else {
            (String::new(), false, value.as_str())
        };

        let mut components = Vec::new();
        for component in remainder.split('/') {
            match component {
                "" | "." => {}
                ".." => {
                    if components.last().is_some_and(|last| last != "..") {
                        components.pop();
                    } else if !absolute {
                        components.push(OsString::from(".."));
                    }
                }
                _ => components.push(OsString::from(component)),
            }
        }
        NormalizedPath {
            prefix: OsString::from(prefix),
            absolute,
            components,
        }
    } else {
        NormalizedPath {
            prefix: OsString::new(),
            absolute: false,
            components: vec![path.as_os_str().to_os_string()],
        }
    }
}

#[cfg(test)]
mod tests {
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

        assert!(
            error.to_string().contains("missing") || error.to_string().contains("No such file")
        );
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
}
