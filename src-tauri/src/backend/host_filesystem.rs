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
#[path = "host_filesystem_tests.rs"]
mod tests;
