use std::path::{Path, PathBuf};

pub const NO_TRAMPOLINE_ENV: &str = "ASSETIWEAVE_NO_TRAMPOLINE";
pub const ENV_KEY: &str = "ASSETIWEAVE_ENV";
pub const DEV_KEY: &str = "ASSETIWEAVE_DEV";

/// Checks if development mode is enabled via environment variables.
pub fn is_development_mode() -> bool {
    let env = std::env::var(ENV_KEY).ok();
    let dev = std::env::var(DEV_KEY).ok();
    is_development_mode_values(env.as_deref(), dev.as_deref())
}

pub(crate) fn is_development_mode_values(env: Option<&str>, dev: Option<&str>) -> bool {
    if let Some(env) = env {
        let trimmed = env.trim().to_lowercase();
        if trimmed == "development" || trimmed == "dev" {
            return true;
        }
        if trimmed == "production" || trimmed == "installed" {
            return false;
        }
    }
    if let Some(dev) = dev {
        let trimmed = dev.trim().to_lowercase();
        if trimmed == "1" || trimmed == "true" {
            return true;
        }
    }
    false
}

/// Checks if trampoline is explicitly disabled via env var.
pub fn is_no_trampoline() -> bool {
    std::env::var(NO_TRAMPOLINE_ENV)
        .map(|v| v.trim() == "1")
        .unwrap_or(false)
}

/// Check if a directory is an AssetIWeave workspace root.
pub fn is_assetiweave_workspace(dir: &Path) -> bool {
    let pkg_json = dir.join("package.json");
    if let Ok(content) = std::fs::read_to_string(&pkg_json) {
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(&content) {
            if val.get("name").and_then(|n| n.as_str()) == Some("assetiweave") {
                return true;
            }
        }
    }

    if dir.join("backend").join("cli").join("Cargo.toml").is_file()
        && dir.join("backend").join("asset_core").is_dir()
    {
        return true;
    }

    false
}

/// Cross-platform executable name helper.
pub fn executable_name(name: &str) -> String {
    #[cfg(windows)]
    {
        if name.ends_with(".exe") {
            name.to_string()
        } else {
            format!("{}.exe", name)
        }
    }
    #[cfg(not(windows))]
    {
        name.to_string()
    }
}

/// Locate workspace CLI starting from a specific directory, traversing upwards.
pub fn find_workspace_cli_from(start_dir: &Path) -> Option<PathBuf> {
    let mut current = start_dir.to_path_buf();
    loop {
        if is_assetiweave_workspace(&current) {
            let candidates = [
                current
                    .join("target")
                    .join("debug")
                    .join(executable_name("aiwc")),
                current
                    .join("target")
                    .join("debug")
                    .join(executable_name("assetiweave-cli")),
            ];
            for candidate in candidates {
                if candidate.is_file() {
                    return Some(candidate);
                }
            }
            return None;
        }

        if !current.pop() {
            break;
        }
    }
    None
}

/// Locate workspace CLI starting from current working directory.
pub fn find_workspace_cli() -> Option<PathBuf> {
    let cwd = std::env::current_dir().ok()?;
    find_workspace_cli_from(&cwd)
}

/// Trampoline to workspace debug CLI if currently running in development mode
/// and current executable is not already the workspace CLI.
pub fn maybe_trampoline() -> Option<i32> {
    if is_no_trampoline() {
        return None;
    }
    if !is_development_mode() {
        return None;
    }

    let ws_cli = find_workspace_cli()?;
    let self_exe = std::env::current_exe().ok()?;

    let real_self = std::fs::canonicalize(&self_exe).unwrap_or(self_exe);
    let real_ws = std::fs::canonicalize(&ws_cli).unwrap_or_else(|_| ws_cli.clone());

    if real_self == real_ws {
        return None;
    }

    std::env::set_var(NO_TRAMPOLINE_ENV, "1");

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let err = std::process::Command::new(&ws_cli)
            .args(std::env::args_os().skip(1))
            .exec();
        eprintln!(
            "aiwc: trampoline warning: exec failed ({}), falling back to spawn",
            err
        );
    }

    let status = std::process::Command::new(&ws_cli)
        .args(std::env::args_os().skip(1))
        .status()
        .ok()?;
    Some(status.code().unwrap_or(1))
}

#[cfg(test)]
#[path = "trampoline_tests.rs"]
mod tests;
