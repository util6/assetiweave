use crate::backend::dto::AppResult;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{
    env,
    ffi::{OsStr, OsString},
    fs,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    thread,
    time::{Duration, Instant},
};

const OPENCODE_COMMAND: &str = "opencode";

#[derive(Debug, Serialize)]
pub(crate) struct OpencodeTranslationAvailability {
    pub(crate) available: bool,
    pub(crate) version: Option<String>,
    pub(crate) error: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct OpencodeTranslationRequest {
    pub(crate) prompt: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct OpencodeTranslationResult {
    pub(crate) translated_text: String,
}

pub(crate) fn check_opencode_translation_availability() -> OpencodeTranslationAvailability {
    match run_opencode_command(&["--version"], Duration::from_secs(8)) {
        Ok(output) if output.status.success() => OpencodeTranslationAvailability {
            available: true,
            version: first_nonempty_line(&output.stdout)
                .or_else(|| first_nonempty_line(&output.stderr)),
            error: None,
        },
        Ok(output) => OpencodeTranslationAvailability {
            available: false,
            version: None,
            error: Some(command_failure_message("opencode --version", &output)),
        },
        Err(error) => OpencodeTranslationAvailability {
            available: false,
            version: None,
            error: Some(error),
        },
    }
}

pub(crate) fn translate_conversation_card_with_opencode(
    params: OpencodeTranslationRequest,
) -> AppResult<OpencodeTranslationResult> {
    let prompt = params.prompt.trim();
    if prompt.is_empty() {
        return Err("translation prompt is empty".to_string());
    }
    if prompt.len() > 200_000 {
        return Err("translation prompt is too large".to_string());
    }

    let output = run_opencode_command(&["run", prompt], Duration::from_secs(180))?;
    if !output.status.success() {
        return Err(command_failure_message("opencode run", &output));
    }

    let translated_text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if translated_text.is_empty() {
        return Err("opencode returned an empty translation".to_string());
    }

    Ok(OpencodeTranslationResult { translated_text })
}

fn run_opencode_command(args: &[&str], timeout: Duration) -> AppResult<Output> {
    let program = resolve_opencode_executable()?;
    run_command_with_timeout(&program, args, timeout)
}

fn resolve_opencode_executable() -> AppResult<PathBuf> {
    let path_env = env::var_os("PATH");
    let login_shell_candidate = find_opencode_with_login_shell();
    let home_dir = dirs::home_dir();
    let search_candidates = opencode_search_candidates(home_dir.as_deref());
    resolve_opencode_executable_from_sources(
        path_env.as_deref(),
        login_shell_candidate,
        &search_candidates,
    )
}

fn resolve_opencode_executable_from_sources(
    path_env: Option<&OsStr>,
    login_shell_candidate: Option<PathBuf>,
    search_candidates: &[PathBuf],
) -> AppResult<PathBuf> {
    if let Some(path) = find_program_on_path(OPENCODE_COMMAND, path_env) {
        return Ok(path);
    }

    if let Some(path) = login_shell_candidate.filter(|path| is_executable_file(path)) {
        return Ok(path);
    }

    for candidate in search_candidates {
        if is_executable_file(candidate) {
            return Ok(candidate.clone());
        }
    }

    Err("opencode was not found on this host. Install OpenCode and make `opencode` available on PATH or from a login shell.".to_string())
}

fn find_program_on_path(program: &str, path_env: Option<&OsStr>) -> Option<PathBuf> {
    let path_env = path_env?;
    for directory in env::split_paths(path_env) {
        if directory.as_os_str().is_empty() {
            continue;
        }
        for file_name in executable_file_names(program) {
            let candidate = directory.join(file_name);
            if is_executable_file(&candidate) {
                return Some(candidate);
            }
        }
    }
    None
}

#[cfg(not(windows))]
fn executable_file_names(program: &str) -> Vec<OsString> {
    vec![OsString::from(program)]
}

#[cfg(windows)]
fn executable_file_names(program: &str) -> Vec<OsString> {
    let program_path = Path::new(program);
    if program_path.extension().is_some() {
        return vec![OsString::from(program)];
    }

    ["exe", "cmd", "bat", "com"]
        .into_iter()
        .map(|extension| OsString::from(format!("{program}.{extension}")))
        .collect()
}

#[cfg(not(windows))]
fn opencode_executable_name() -> &'static str {
    OPENCODE_COMMAND
}

#[cfg(windows)]
fn opencode_executable_name() -> &'static str {
    "opencode.exe"
}

fn opencode_search_candidates(home_dir: Option<&Path>) -> Vec<PathBuf> {
    let executable = opencode_executable_name();
    let mut candidates = Vec::new();

    #[cfg(not(windows))]
    candidates.extend([
        Path::new("/opt/homebrew/bin").join(executable),
        Path::new("/usr/local/bin").join(executable),
        Path::new("/opt/local/bin").join(executable),
    ]);

    if let Some(home_dir) = home_dir {
        candidates.extend([
            home_dir.join(".opencode").join("bin").join(executable),
            home_dir.join(".local").join("bin").join(executable),
            home_dir.join(".npm-global").join("bin").join(executable),
            home_dir.join(".pnpm-global").join("bin").join(executable),
            home_dir.join(".bun").join("bin").join(executable),
            home_dir.join(".deno").join("bin").join(executable),
            home_dir.join(".cargo").join("bin").join(executable),
            home_dir.join(".volta").join("bin").join(executable),
            home_dir.join("Library").join("pnpm").join(executable),
        ]);
    }

    candidates
}

#[cfg(not(windows))]
fn find_opencode_with_login_shell() -> Option<PathBuf> {
    let shell = login_shell()?;
    let output = Command::new(shell)
        .args(["-lc", "command -v opencode"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }

    let path = PathBuf::from(first_nonempty_line(&output.stdout)?);
    if path.is_absolute() && is_executable_file(&path) {
        Some(path)
    } else {
        None
    }
}

#[cfg(windows)]
fn find_opencode_with_login_shell() -> Option<PathBuf> {
    None
}

#[cfg(not(windows))]
fn login_shell() -> Option<PathBuf> {
    env::var_os("SHELL")
        .map(PathBuf::from)
        .filter(|path| is_executable_file(path))
        .or_else(|| {
            ["/bin/zsh", "/bin/bash", "/bin/sh"]
                .into_iter()
                .map(PathBuf::from)
                .find(|path| is_executable_file(path))
        })
}

fn is_executable_file(path: &Path) -> bool {
    let Ok(metadata) = fs::metadata(path) else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        metadata.permissions().mode() & 0o111 != 0
    }

    #[cfg(not(unix))]
    {
        true
    }
}

fn run_command_with_timeout(program: &Path, args: &[&str], timeout: Duration) -> AppResult<Output> {
    let mut child = Command::new(program)
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("failed to start {}: {error}", program.display()))?;
    let started_at = Instant::now();

    loop {
        match child.try_wait().map_err(|error| error.to_string())? {
            Some(_) => return child.wait_with_output().map_err(|error| error.to_string()),
            None if started_at.elapsed() >= timeout => {
                let _ = child.kill();
                let _ = child.wait_with_output();
                return Err(format!(
                    "{} timed out after {} seconds",
                    program.display(),
                    timeout.as_secs()
                ));
            }
            None => thread::sleep(Duration::from_millis(50)),
        }
    }
}

fn first_nonempty_line(bytes: &[u8]) -> Option<String> {
    String::from_utf8_lossy(bytes)
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(str::to_string)
}

fn command_failure_message(command_name: &str, output: &Output) -> String {
    let detail = first_nonempty_line(&output.stderr)
        .or_else(|| first_nonempty_line(&output.stdout))
        .unwrap_or_else(|| output.status.to_string());
    format!("{command_name} failed: {detail}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{env, fs, path::Path};

    struct TempDir {
        path: std::path::PathBuf,
    }

    impl TempDir {
        fn new(prefix: &str) -> Self {
            let path = env::temp_dir().join(format!("{prefix}-{}", uuid::Uuid::new_v4()));
            fs::create_dir_all(&path).unwrap();
            Self { path }
        }

        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn resolves_opencode_from_path() {
        let dir = TempDir::new("assetiweave-opencode-path");
        let executable = dir.path().join(opencode_executable_name());
        write_executable(&executable);
        let path_env = env::join_paths([dir.path()]).unwrap();

        let resolved =
            resolve_opencode_executable_from_sources(Some(path_env.as_os_str()), None, &[])
                .unwrap();

        assert_eq!(resolved, executable);
    }

    #[test]
    fn resolves_opencode_from_search_candidates_when_path_is_empty() {
        let dir = TempDir::new("assetiweave-opencode-candidate");
        let executable = dir.path().join(opencode_executable_name());
        write_executable(&executable);

        let resolved = resolve_opencode_executable_from_sources(
            Some(std::ffi::OsStr::new("")),
            None,
            &[executable.clone()],
        )
        .unwrap();

        assert_eq!(resolved, executable);
    }

    #[test]
    fn ignores_missing_login_shell_candidate() {
        let dir = TempDir::new("assetiweave-opencode-login-shell");
        let fallback = dir.path().join(opencode_executable_name());
        write_executable(&fallback);

        let resolved = resolve_opencode_executable_from_sources(
            Some(std::ffi::OsStr::new("")),
            Some(dir.path().join("missing-opencode")),
            &[fallback.clone()],
        )
        .unwrap();

        assert_eq!(resolved, fallback);
    }

    #[test]
    #[cfg(not(windows))]
    fn includes_host_install_locations_in_search_candidates() {
        let home = Path::new("/Users/example");
        let candidates = opencode_search_candidates(Some(home));

        assert!(
            candidates.contains(&Path::new("/opt/homebrew/bin").join(opencode_executable_name()))
        );
        assert!(candidates.contains(&Path::new("/usr/local/bin").join(opencode_executable_name())));
        assert!(candidates.contains(
            &home
                .join(".opencode")
                .join("bin")
                .join(opencode_executable_name())
        ));
        assert!(candidates.contains(
            &home
                .join(".local")
                .join("bin")
                .join(opencode_executable_name())
        ));
    }

    fn write_executable(path: &Path) {
        fs::write(path, "#!/bin/sh\nexit 0\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut permissions = fs::metadata(path).unwrap().permissions();
            permissions.set_mode(0o755);
            fs::set_permissions(path, permissions).unwrap();
        }
    }
}
