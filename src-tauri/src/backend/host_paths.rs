use crate::backend::runtime::AppResult;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub(crate) enum HostPlatform {
    Macos,
    Windows,
    Linux,
}

impl HostPlatform {
    pub(crate) fn current() -> Self {
        #[cfg(target_os = "macos")]
        {
            Self::Macos
        }
        #[cfg(target_os = "windows")]
        {
            Self::Windows
        }
        #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
        {
            Self::Linux
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct HostDirectories {
    pub(crate) home: PathBuf,
    pub(crate) config: PathBuf,
    pub(crate) local_data: PathBuf,
    pub(crate) data: PathBuf,
    pub(crate) cache: PathBuf,
    pub(crate) workspace: PathBuf,
}

impl HostDirectories {
    pub(crate) fn current() -> AppResult<Self> {
        let base_dirs = directories::BaseDirs::new();
        let home = base_dirs
            .as_ref()
            .map(|b| b.home_dir().to_path_buf())
            .or_else(dirs::home_dir)
            .or_else(|| std::env::var_os("USERPROFILE").map(PathBuf::from))
            .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
            .ok_or_else(|| {
                crate::backend::runtime::AppError::NotFound("无法确定用户基本目录".to_string())
            })?;

        let config = base_dirs
            .as_ref()
            .map(|b| b.config_dir().to_path_buf())
            .or_else(dirs::config_dir)
            .or_else(|| std::env::var_os("APPDATA").map(PathBuf::from))
            .unwrap_or_else(|| {
                #[cfg(target_os = "windows")]
                {
                    home.join("AppData").join("Roaming")
                }
                #[cfg(not(target_os = "windows"))]
                {
                    home.join(".config")
                }
            });

        let local_data = base_dirs
            .as_ref()
            .map(|b| b.data_local_dir().to_path_buf())
            .or_else(dirs::data_local_dir)
            .or_else(|| std::env::var_os("LOCALAPPDATA").map(PathBuf::from))
            .unwrap_or_else(|| {
                #[cfg(target_os = "windows")]
                {
                    home.join("AppData").join("Local")
                }
                #[cfg(target_os = "macos")]
                {
                    home.join("Library").join("Application Support")
                }
                #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
                {
                    home.join(".local").join("share")
                }
            });

        let data = base_dirs
            .as_ref()
            .map(|b| b.data_dir().to_path_buf())
            .or_else(dirs::data_dir)
            .unwrap_or_else(|| {
                #[cfg(target_os = "windows")]
                {
                    config.clone()
                }
                #[cfg(not(target_os = "windows"))]
                {
                    local_data.clone()
                }
            });

        let cache = base_dirs
            .as_ref()
            .map(|b| b.cache_dir().to_path_buf())
            .or_else(dirs::cache_dir)
            .unwrap_or_else(|| {
                #[cfg(target_os = "windows")]
                {
                    home.join("AppData").join("Local")
                }
                #[cfg(target_os = "macos")]
                {
                    home.join("Library").join("Caches")
                }
                #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
                {
                    home.join(".cache")
                }
            });

        let workspace = std::env::current_dir()
            .map_err(|error| crate::backend::runtime::AppError::External(error.to_string()))?;

        Ok(Self {
            home,
            config,
            local_data,
            data,
            cache,
            workspace,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct StoredPath(String);

impl StoredPath {
    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResolvedPath(PathBuf);

impl ResolvedPath {
    pub(crate) fn as_path(&self) -> &Path {
        &self.0
    }

    pub(crate) fn into_path_buf(self) -> PathBuf {
        self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct DisplayPath(String);

impl DisplayPath {
    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PathAnchor {
    Home,
    Config,
    LocalData,
    Data,
    Cache,
    Workspace,
    Absolute,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct PathSpec {
    anchor: PathAnchor,
    value: String,
}

#[derive(Debug, Clone)]
pub(crate) struct HostPathResolver {
    platform: HostPlatform,
    directories: HostDirectories,
}

impl HostPathResolver {
    pub(crate) fn current() -> AppResult<Self> {
        Ok(Self::new(
            HostPlatform::current(),
            HostDirectories::current()?,
        ))
    }

    pub(crate) fn new(platform: HostPlatform, directories: HostDirectories) -> Self {
        Self {
            platform,
            directories,
        }
    }

    pub(crate) fn normalize_input(&self, raw: &str) -> AppResult<StoredPath> {
        let spec = self.parse(raw)?;
        if spec.anchor == PathAnchor::Absolute {
            for (anchor, directory) in [
                ("@cache", &self.directories.cache),
                ("@config", &self.directories.config),
                ("@local-data", &self.directories.local_data),
                ("@data", &self.directories.data),
            ] {
                if self.same_directory(directory, &self.directories.home) {
                    continue;
                }
                if let Some(relative) = self.strip_directory_prefix(&spec.value, directory) {
                    return Ok(StoredPath(format_anchored(anchor, &relative)));
                }
            }
            if let Some(relative) = self.strip_directory_prefix(&spec.value, &self.directories.home)
            {
                return Ok(StoredPath(format_anchored("~", &relative)));
            }
        }
        Ok(StoredPath(self.format_spec(&spec)))
    }

    pub(crate) fn resolve(&self, stored: &StoredPath) -> AppResult<ResolvedPath> {
        let spec = self.parse(stored.as_str())?;
        let path = match spec.anchor {
            PathAnchor::Home => self.join(&self.directories.home, &spec.value),
            PathAnchor::Config => self.join(&self.directories.config, &spec.value),
            PathAnchor::LocalData => self.join(&self.directories.local_data, &spec.value),
            PathAnchor::Data => self.join(&self.directories.data, &spec.value),
            PathAnchor::Cache => self.join(&self.directories.cache, &spec.value),
            PathAnchor::Workspace => {
                let direct = self.join(&self.directories.workspace, &spec.value);
                if direct.exists() {
                    direct
                } else if let Some(parent) = self.directories.workspace.parent() {
                    let parent_candidate = self.join(parent, &spec.value);
                    if parent_candidate.exists() {
                        parent_candidate
                    } else {
                        direct
                    }
                } else {
                    direct
                }
            }
            PathAnchor::Absolute => PathBuf::from(spec.value),
        };
        Ok(ResolvedPath(path))
    }

    pub(crate) fn display(&self, stored: &StoredPath) -> AppResult<DisplayPath> {
        let resolved = self.resolve(stored)?;
        if let Some(relative) = self.strip_directory_prefix(
            &resolved.as_path().to_string_lossy(),
            &self.directories.home,
        ) {
            return Ok(DisplayPath(format_anchored("~", &relative)));
        }
        Ok(DisplayPath(
            self.portable_text(&resolved.as_path().to_string_lossy()),
        ))
    }

    fn parse(&self, raw: &str) -> AppResult<PathSpec> {
        let raw = raw.trim();
        if raw.is_empty() {
            return Err(crate::backend::runtime::AppError::Validation(
                "path must not be empty".to_string(),
            ));
        }

        for (prefix, anchor) in [
            ("~", PathAnchor::Home),
            ("@config", PathAnchor::Config),
            ("@local-data", PathAnchor::LocalData),
            ("@data", PathAnchor::Data),
            ("@cache", PathAnchor::Cache),
            ("%USERPROFILE%", PathAnchor::Home),
            ("%APPDATA%", PathAnchor::Config),
            ("%LOCALAPPDATA%", PathAnchor::LocalData),
        ] {
            if let Some(relative) = strip_anchor(raw, prefix) {
                return Ok(PathSpec {
                    anchor,
                    value: self.portable_relative_text(relative),
                });
            }
        }

        if Path::new(raw).is_absolute()
            || raw.starts_with('/')
            || looks_like_windows_absolute_path(raw)
        {
            return Ok(PathSpec {
                anchor: PathAnchor::Absolute,
                value: self.host_text(raw),
            });
        }

        Ok(PathSpec {
            anchor: PathAnchor::Workspace,
            value: self.portable_relative_text(raw),
        })
    }

    fn format_spec(&self, spec: &PathSpec) -> String {
        match spec.anchor {
            PathAnchor::Home => format_anchored("~", &spec.value),
            PathAnchor::Config => format_anchored("@config", &spec.value),
            PathAnchor::LocalData => format_anchored("@local-data", &spec.value),
            PathAnchor::Data => format_anchored("@data", &spec.value),
            PathAnchor::Cache => format_anchored("@cache", &spec.value),
            PathAnchor::Workspace | PathAnchor::Absolute => self.portable_text(&spec.value),
        }
    }

    fn join(&self, base: &Path, relative: &str) -> PathBuf {
        if relative.is_empty() {
            return base.to_path_buf();
        }
        if self.platform == HostPlatform::Windows {
            if let Some(base_str) = base.to_str() {
                let base = base_str.replace('/', "\\");
                let relative = relative.replace('/', "\\");
                return PathBuf::from(format!(
                    "{}\\{}",
                    base.trim_end_matches(['\\', '/']),
                    relative.trim_start_matches(['\\', '/'])
                ));
            }
        }
        let mut result = base.to_path_buf();
        for part in relative.split(['/', '\\']) {
            if !part.is_empty() {
                result.push(part);
            }
        }
        result
    }

    fn strip_directory_prefix(&self, path: &str, directory: &Path) -> Option<String> {
        let directory_str = directory.to_str()?;
        let path = self.portable_text(path);
        let directory = self.portable_text(directory_str);
        let comparison_path = self.comparison_text(&path);
        let comparison_directory = self.comparison_text(directory.trim_end_matches('/'));
        if comparison_path == comparison_directory {
            return Some(String::new());
        }
        let prefix = format!("{comparison_directory}/");
        comparison_path
            .starts_with(&prefix)
            .then(|| path[prefix.len()..].to_string())
    }

    fn comparison_text(&self, value: &str) -> String {
        if self.platform == HostPlatform::Windows {
            value.to_ascii_lowercase()
        } else {
            value.to_string()
        }
    }

    fn same_directory(&self, left: &Path, right: &Path) -> bool {
        crate::backend::host_filesystem::HostFilesystem::new(self.platform).same_path(left, right)
    }

    fn host_text(&self, value: &str) -> String {
        if self.platform == HostPlatform::Windows {
            value.replace('/', "\\")
        } else {
            value.to_string()
        }
    }

    fn portable_text(&self, value: &str) -> String {
        value.replace('\\', "/")
    }

    fn portable_relative_text(&self, value: &str) -> String {
        self.portable_text(value)
            .trim_start_matches('/')
            .trim_end_matches('/')
            .to_string()
    }
}

fn strip_anchor<'a>(raw: &'a str, prefix: &str) -> Option<&'a str> {
    if raw.eq_ignore_ascii_case(prefix) {
        return Some("");
    }
    let suffix = raw.get(prefix.len()..)?;
    raw.get(..prefix.len())
        .is_some_and(|candidate| candidate.eq_ignore_ascii_case(prefix))
        .then_some(suffix)
        .filter(|suffix| suffix.starts_with(['/', '\\']))
        .map(|suffix| suffix.trim_start_matches(['/', '\\']))
}

fn format_anchored(anchor: &str, relative: &str) -> String {
    if relative.is_empty() {
        anchor.to_string()
    } else {
        format!("{anchor}/{}", relative.trim_start_matches('/'))
    }
}

fn looks_like_windows_absolute_path(path: &str) -> bool {
    let bytes = path.as_bytes();
    path.starts_with("\\\\")
        || path.starts_with("//")
        || (bytes.len() >= 3
            && bytes[0].is_ascii_alphabetic()
            && bytes[1] == b':'
            && matches!(bytes[2], b'\\' | b'/'))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    #[test]
    fn normalizes_absolute_home_paths_to_portable_home_storage() {
        let resolver = macos_resolver();

        let stored = resolver
            .normalize_input("/Users/alice/.codex/skills")
            .expect("normalize home path");

        assert_eq!(stored.as_str(), "~/.codex/skills");
        assert_eq!(
            resolver
                .resolve(&stored)
                .expect("resolve home path")
                .as_path(),
            Path::new("/Users/alice/.codex/skills")
        );
        assert_eq!(
            resolver
                .display(&stored)
                .expect("display home path")
                .as_str(),
            "~/.codex/skills"
        );
    }

    #[test]
    fn windows_home_paths_are_compared_case_insensitively_and_use_forward_slashes_in_storage() {
        let resolver = windows_resolver();

        let stored = resolver
            .normalize_input(r"c:\USERS\ALICE\.codex\skills")
            .expect("normalize Windows home path");

        assert_eq!(stored.as_str(), "~/.codex/skills");
    }

    #[test]
    fn windows_appdata_alias_resolves_through_config_anchor_but_displays_under_home() {
        let resolver = windows_resolver();

        let stored = resolver
            .normalize_input(r"%APPDATA%\Cursor\skills")
            .expect("normalize APPDATA path");

        assert_eq!(stored.as_str(), "@config/Cursor/skills");
        assert_eq!(
            resolver
                .resolve(&stored)
                .expect("resolve config path")
                .as_path(),
            Path::new(r"C:\Users\Alice\AppData\Roaming\Cursor\skills")
        );
        assert_eq!(
            resolver
                .display(&stored)
                .expect("display config path")
                .as_str(),
            "~/AppData/Roaming/Cursor/skills"
        );
    }

    #[test]
    fn absolute_platform_config_paths_normalize_to_config_anchor_before_home() {
        let macos = macos_resolver();
        let windows = windows_resolver();

        assert_eq!(
            macos
                .normalize_input(
                    "/Users/alice/Library/Application Support/assetiweave/conversation-adapters"
                )
                .expect("normalize macOS config path")
                .as_str(),
            "@config/assetiweave/conversation-adapters"
        );
        assert_eq!(
            windows
                .normalize_input(
                    r"C:\Users\Alice\AppData\Roaming\assetiweave\conversation-adapters"
                )
                .expect("normalize Windows config path")
                .as_str(),
            "@config/assetiweave/conversation-adapters"
        );
    }

    #[test]
    fn absolute_paths_outside_home_remain_absolute() {
        let resolver = windows_resolver();

        let stored = resolver
            .normalize_input(r"D:\Shared\skills")
            .expect("normalize external path");

        assert_eq!(stored.as_str(), "D:/Shared/skills");
    }

    #[test]
    fn relative_paths_resolve_from_workspace_and_remain_relative_in_storage() {
        let resolver = macos_resolver();

        let stored = resolver
            .normalize_input("agent-docs/feature-plans/runtime-extension-refactor/00-overview.md")
            .expect("normalize workspace path");

        assert_eq!(
            stored.as_str(),
            "agent-docs/feature-plans/runtime-extension-refactor/00-overview.md"
        );
        assert_eq!(
            resolver
                .resolve(&stored)
                .expect("resolve workspace path")
                .as_path(),
            Path::new("/workspace/assetiweave/agent-docs/feature-plans/runtime-extension-refactor/00-overview.md")
        );
    }

    #[test]
    fn linux_all_anchors_normalize_resolve_and_display_round_trip() {
        let resolver = linux_resolver();

        // Config
        let config_stored = resolver
            .normalize_input("/home/alice/.config/assetiweave/skills")
            .expect("normalize linux config path");
        assert_eq!(config_stored.as_str(), "@config/assetiweave/skills");
        assert_eq!(
            resolver.resolve(&config_stored).expect("resolve").as_path(),
            Path::new("/home/alice/.config/assetiweave/skills")
        );
        assert_eq!(
            resolver.display(&config_stored).expect("display").as_str(),
            "~/.config/assetiweave/skills"
        );

        // Data / LocalData
        let data_stored = resolver
            .normalize_input("/home/alice/.local/share/assetiweave/data")
            .expect("normalize linux data path");
        assert_eq!(data_stored.as_str(), "@local-data/assetiweave/data");
        assert_eq!(
            resolver.resolve(&data_stored).expect("resolve").as_path(),
            Path::new("/home/alice/.local/share/assetiweave/data")
        );

        // Cache
        let cache_stored = resolver
            .normalize_input("/home/alice/.cache/assetiweave/cache")
            .expect("normalize linux cache path");
        assert_eq!(cache_stored.as_str(), "@cache/assetiweave/cache");
        assert_eq!(
            resolver.resolve(&cache_stored).expect("resolve").as_path(),
            Path::new("/home/alice/.cache/assetiweave/cache")
        );

        // Home
        let home_stored = resolver
            .normalize_input("/home/alice/projects/demo")
            .expect("normalize linux home path");
        assert_eq!(home_stored.as_str(), "~/projects/demo");
        assert_eq!(
            resolver.resolve(&home_stored).expect("resolve").as_path(),
            Path::new("/home/alice/projects/demo")
        );
        assert_eq!(
            resolver.display(&home_stored).expect("display").as_str(),
            "~/projects/demo"
        );
    }

    #[test]
    fn longest_prefix_prefers_specific_anchor_over_home() {
        let resolver = macos_resolver();

        let stored = resolver
            .normalize_input("/Users/alice/Library/Caches/assetiweave/logs")
            .expect("normalize cache path");
        // Should match @cache rather than ~
        assert_eq!(stored.as_str(), "@cache/assetiweave/logs");
    }

    #[test]
    fn relative_traversal_paths_are_kept_portable() {
        let resolver = macos_resolver();

        let stored = resolver
            .normalize_input("src/../docs/guide.md")
            .expect("normalize relative traversal path");
        assert_eq!(stored.as_str(), "src/../docs/guide.md");
    }

    fn macos_resolver() -> HostPathResolver {
        HostPathResolver::new(
            HostPlatform::Macos,
            HostDirectories {
                home: PathBuf::from("/Users/alice"),
                config: PathBuf::from("/Users/alice/Library/Application Support"),
                local_data: PathBuf::from("/Users/alice/Library/Application Support"),
                data: PathBuf::from("/Users/alice/Library/Application Support"),
                cache: PathBuf::from("/Users/alice/Library/Caches"),
                workspace: PathBuf::from("/workspace/assetiweave"),
            },
        )
    }

    fn windows_resolver() -> HostPathResolver {
        HostPathResolver::new(
            HostPlatform::Windows,
            HostDirectories {
                home: PathBuf::from(r"C:\Users\Alice"),
                config: PathBuf::from(r"C:\Users\Alice\AppData\Roaming"),
                local_data: PathBuf::from(r"C:\Users\Alice\AppData\Local"),
                data: PathBuf::from(r"C:\Users\Alice\AppData\Roaming"),
                cache: PathBuf::from(r"C:\Users\Alice\AppData\Local\Cache"),
                workspace: PathBuf::from(r"C:\workspace\assetiweave"),
            },
        )
    }

    fn linux_resolver() -> HostPathResolver {
        HostPathResolver::new(
            HostPlatform::Linux,
            HostDirectories {
                home: PathBuf::from("/home/alice"),
                config: PathBuf::from("/home/alice/.config"),
                local_data: PathBuf::from("/home/alice/.local/share"),
                data: PathBuf::from("/home/alice/.local/share"),
                cache: PathBuf::from("/home/alice/.cache"),
                workspace: PathBuf::from("/workspace/assetiweave"),
            },
        )
    }
}
