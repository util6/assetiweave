use std::path::{Path, PathBuf};

use crate::backend::store::{StoreError, StoreResult};

/// Folds an absolute path to a portable representation (e.g. replacing home with `~`) for storage.
pub fn normalize_path_for_storage(raw: &str) -> StoreResult<String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(String::new());
    }

    // Already anchored
    for prefix in [
        "~",
        "@config",
        "@local-data",
        "@data",
        "@cache",
        "%USERPROFILE%",
        "%APPDATA%",
        "%LOCALAPPDATA%",
    ] {
        if let Some(relative) = strip_anchor(trimmed, prefix) {
            let canonical_anchor = match prefix {
                "%USERPROFILE%" => "~",
                "%APPDATA%" => "@config",
                "%LOCALAPPDATA%" => "@local-data",
                other => other,
            };
            return Ok(format_anchored(canonical_anchor, &portable_text(relative)));
        }
    }

    // Check directory prefixes
    let base_dirs = directories::BaseDirs::new();
    let home = base_dirs
        .as_ref()
        .map(|b| b.home_dir().to_path_buf())
        .or_else(dirs::home_dir);

    let config = base_dirs
        .as_ref()
        .map(|b| b.config_dir().to_path_buf())
        .or_else(dirs::config_dir);

    let local_data = base_dirs
        .as_ref()
        .map(|b| b.data_local_dir().to_path_buf())
        .or_else(dirs::data_local_dir);

    let data = base_dirs
        .as_ref()
        .map(|b| b.data_dir().to_path_buf())
        .or_else(dirs::data_dir);

    let cache = base_dirs
        .as_ref()
        .map(|b| b.cache_dir().to_path_buf())
        .or_else(dirs::cache_dir);

    // Try specific non-home directories first if distinct from home
    for (anchor, dir_opt) in [
        ("@cache", cache.as_deref()),
        ("@config", config.as_deref()),
        ("@local-data", local_data.as_deref()),
        ("@data", data.as_deref()),
    ] {
        if let Some(dir) = dir_opt {
            if let Some(home_dir) = home.as_deref() {
                if same_directory(dir, home_dir) {
                    continue;
                }
            }
            if let Some(relative) = strip_directory_prefix(trimmed, dir) {
                return Ok(format_anchored(anchor, &relative));
            }
        }
    }

    // Check home directory
    if let Some(home_dir) = home.as_deref() {
        if let Some(relative) = strip_directory_prefix(trimmed, home_dir) {
            return Ok(format_anchored("~", &relative));
        }
    }

    Ok(portable_text(trimmed))
}

/// Expands a stored path (e.g. beginning with `~`, `@config`, etc.) back to an absolute PathBuf.
pub fn expand_path(raw: &str) -> StoreResult<PathBuf> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(PathBuf::new());
    }

    for (prefix, resolver) in [
        (
            "~",
            Box::new(|| dirs::home_dir()) as Box<dyn Fn() -> Option<PathBuf>>,
        ),
        ("%USERPROFILE%", Box::new(|| dirs::home_dir())),
        (
            "@config",
            Box::new(|| {
                directories::BaseDirs::new()
                    .map(|b| b.config_dir().to_path_buf())
                    .or_else(dirs::config_dir)
            }),
        ),
        (
            "%APPDATA%",
            Box::new(|| {
                directories::BaseDirs::new()
                    .map(|b| b.config_dir().to_path_buf())
                    .or_else(dirs::config_dir)
            }),
        ),
        (
            "@local-data",
            Box::new(|| {
                directories::BaseDirs::new()
                    .map(|b| b.data_local_dir().to_path_buf())
                    .or_else(dirs::data_local_dir)
            }),
        ),
        (
            "%LOCALAPPDATA%",
            Box::new(|| {
                directories::BaseDirs::new()
                    .map(|b| b.data_local_dir().to_path_buf())
                    .or_else(dirs::data_local_dir)
            }),
        ),
        (
            "@data",
            Box::new(|| {
                directories::BaseDirs::new()
                    .map(|b| b.data_dir().to_path_buf())
                    .or_else(dirs::data_dir)
            }),
        ),
        (
            "@cache",
            Box::new(|| {
                directories::BaseDirs::new()
                    .map(|b| b.cache_dir().to_path_buf())
                    .or_else(dirs::cache_dir)
            }),
        ),
    ] {
        if let Some(relative) = strip_anchor(trimmed, prefix) {
            let base = resolver().ok_or_else(|| {
                StoreError::external(format!("failed to determine base directory for {prefix}"))
            })?;
            return Ok(join_relative(&base, relative));
        }
    }

    Ok(PathBuf::from(trimmed))
}

fn portable_text(value: &str) -> String {
    value.replace('\\', "/")
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
    let clean = relative.trim_start_matches(['/', '\\']);
    if clean.is_empty() {
        anchor.to_string()
    } else {
        format!("{anchor}/{clean}")
    }
}

fn strip_directory_prefix(path: &str, directory: &Path) -> Option<String> {
    let directory_str = directory.to_str()?;
    let path_portable = portable_text(path);
    let dir_portable = portable_text(directory_str);
    let comp_path = comparison_text(&path_portable);
    let comp_dir = comparison_text(dir_portable.trim_end_matches('/'));

    if comp_path == comp_dir {
        return Some(String::new());
    }
    let prefix = format!("{comp_dir}/");
    comp_path
        .starts_with(&prefix)
        .then(|| path_portable[prefix.len()..].to_string())
}

fn comparison_text(value: &str) -> String {
    #[cfg(target_os = "windows")]
    {
        value.to_ascii_lowercase()
    }
    #[cfg(not(target_os = "windows"))]
    {
        value.to_string()
    }
}

fn same_directory(left: &Path, right: &Path) -> bool {
    let left_clean = comparison_text(&portable_text(&left.to_string_lossy()));
    let right_clean = comparison_text(&portable_text(&right.to_string_lossy()));
    left_clean.trim_end_matches('/') == right_clean.trim_end_matches('/')
}

fn join_relative(base: &Path, relative: &str) -> PathBuf {
    if relative.is_empty() {
        return base.to_path_buf();
    }
    let mut result = base.to_path_buf();
    for part in relative.split(['/', '\\']) {
        if !part.is_empty() {
            result.push(part);
        }
    }
    result
}

#[cfg(test)]
#[path = "path_codec_tests.rs"]
mod tests;
