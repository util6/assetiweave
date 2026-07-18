use crate::backend::{dto::AppResult, host_paths::HostPlatform};
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
        self.relative_components(path, root).is_some()
    }

    pub(crate) fn relative_components(&self, path: &Path, root: &Path) -> Option<Vec<String>> {
        let path = self.normalized_path(path);
        let root = self.normalized_path(root);
        (path.prefix == root.prefix
            && path.absolute == root.absolute
            && path.components.starts_with(&root.components))
        .then(|| path.components[root.components.len()..].to_vec())
    }

    pub(crate) fn validate_path_segment(&self, segment: &str) -> AppResult<String> {
        if segment.ends_with([' ', '.']) {
            return Err(format!(
                "path segment must not end with a space or period: {segment}"
            ));
        }
        let segment = segment.trim();
        if segment.is_empty() || matches!(segment, "." | "..") {
            return Err("path segment must not be empty, '.' or '..'".to_string());
        }
        if segment
            .chars()
            .any(|character| character.is_control() || r#"<>:"/\|?*"#.contains(character))
        {
            return Err(format!(
                "path segment contains a platform-reserved character: {segment}"
            ));
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
            return Err(format!("path segment is reserved on Windows: {segment}"));
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
            return Err(format!("path must be portable and relative: {raw}"));
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
        let kind = if fs::metadata(source)
            .map_err(|error| error.to_string())?
            .is_dir()
        {
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
        let metadata = fs::symlink_metadata(path).map_err(|error| error.to_string())?;
        symlink_kind(path, &metadata)
    }

    pub(crate) fn remove_symlink(&self, path: &Path) -> AppResult<()> {
        let kind = self.symlink_kind(path)?;
        match symlink_removal(self.platform, kind) {
            SymlinkRemoval::File => fs::remove_file(path),
            SymlinkRemoval::Directory => fs::remove_dir(path),
        }
        .map_err(|error| error.to_string())
    }

    pub(crate) fn remove_path(&self, path: &Path) -> AppResult<()> {
        let metadata = fs::symlink_metadata(path).map_err(|error| error.to_string())?;
        if metadata.file_type().is_symlink() {
            return self.remove_symlink(path);
        }
        if metadata.is_file() {
            return fs::remove_file(path).map_err(|error| error.to_string());
        }
        if metadata.is_dir() {
            return fs::remove_dir_all(path).map_err(|error| error.to_string());
        }
        Err(format!("unsupported filesystem entry: {}", path.display()))
    }

    pub(crate) fn copy_dir(&self, source: &Path, target: &Path) -> AppResult<()> {
        for entry in WalkDir::new(source) {
            let entry = entry.map_err(|error| error.to_string())?;
            let relative = entry
                .path()
                .strip_prefix(source)
                .map_err(|error| error.to_string())?;
            let destination = target.join(relative);
            if entry.file_type().is_dir() {
                fs::create_dir_all(&destination).map_err(|error| error.to_string())?;
            } else if entry.file_type().is_file() {
                if let Some(parent) = destination.parent() {
                    fs::create_dir_all(parent).map_err(|error| error.to_string())?;
                }
                fs::copy(entry.path(), destination).map_err(|error| error.to_string())?;
            }
        }
        Ok(())
    }

    pub(crate) fn copy_dir_without_conflicts(&self, source: &Path, target: &Path) -> AppResult<()> {
        if !source.exists() {
            return Ok(());
        }
        if !source.is_dir() {
            return Err(format!(
                "backup source is not a directory: {}",
                source.display()
            ));
        }

        for entry in WalkDir::new(source) {
            let entry = entry.map_err(|error| error.to_string())?;
            let relative = entry
                .path()
                .strip_prefix(source)
                .map_err(|error| error.to_string())?;
            let destination = target.join(relative);
            if entry.file_type().is_dir() {
                fs::create_dir_all(&destination).map_err(|error| error.to_string())?;
                continue;
            }
            if !entry.file_type().is_file() {
                continue;
            }

            if destination.exists() {
                if !destination.is_file() {
                    return Err(format!(
                        "backup migration target is not a file: {}",
                        destination.display()
                    ));
                }
                let source_bytes = fs::read(entry.path()).map_err(|error| error.to_string())?;
                let destination_bytes =
                    fs::read(&destination).map_err(|error| error.to_string())?;
                if source_bytes != destination_bytes {
                    return Err(format!(
                        "backup migration target already has different content: {}",
                        destination.display()
                    ));
                }
                continue;
            }

            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent).map_err(|error| error.to_string())?;
            }
            fs::copy(entry.path(), destination).map_err(|error| error.to_string())?;
        }
        Ok(())
    }

    fn normalized_path(&self, path: &Path) -> NormalizedPath {
        let path = if self.platform == HostPlatform::current() {
            canonicalize_with_missing_tail(path)
        } else {
            path.to_path_buf()
        };
        NormalizedPath::parse(self.platform, &path.to_string_lossy())
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
    std::os::unix::fs::symlink(source, target).map_err(|error| error.to_string())
}

#[cfg(windows)]
fn create_symlink_with_kind(source: &Path, target: &Path, kind: SymlinkKind) -> AppResult<()> {
    match kind {
        SymlinkKind::Directory => std::os::windows::fs::symlink_dir(source, target),
        SymlinkKind::File => std::os::windows::fs::symlink_file(source, target),
    }
    .map_err(|error| format_symlink_error(HostPlatform::Windows, error))
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
        return Err(format!("target is not a symlink: {}", path.display()));
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
        Err(format!("target is not a symlink: {}", path.display()))
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
    prefix: String,
    absolute: bool,
    components: Vec<String>,
}

impl NormalizedPath {
    fn parse(platform: HostPlatform, raw: &str) -> Self {
        let mut value = raw.replace('\\', "/");
        if platform == HostPlatform::Windows {
            value.make_ascii_lowercase();
        }

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
                        components.push(component.to_string());
                    }
                }
                _ => components.push(component.to_string()),
            }
        }
        Self {
            prefix,
            absolute,
            components,
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

        assert!(error.contains("missing") || error.contains("No such file"));
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
}
