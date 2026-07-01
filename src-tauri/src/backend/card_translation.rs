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
const GEMINI_COMMAND: &str = "gemini";

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

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ConversationTranslationProvider {
    Cli,
    Google,
    Apple,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ConversationTranslationCli {
    Opencode,
    Gemini,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct ConversationTranslationRequest {
    pub(crate) provider: ConversationTranslationProvider,
    pub(crate) cli: ConversationTranslationCli,
    pub(crate) model: String,
    pub(crate) prompt: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct ConversationTranslationConnectionRequest {
    pub(crate) provider: ConversationTranslationProvider,
    pub(crate) cli: ConversationTranslationCli,
    pub(crate) model: String,
    pub(crate) prompt: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct ConversationTranslationModelsRequest {
    pub(crate) provider: ConversationTranslationProvider,
    pub(crate) cli: ConversationTranslationCli,
}

#[derive(Debug, Serialize)]
pub(crate) struct ConversationTranslationModelsResult {
    pub(crate) models: Vec<String>,
    pub(crate) error: Option<String>,
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

pub(crate) fn test_conversation_translation_connection(
    params: ConversationTranslationConnectionRequest,
) -> OpencodeTranslationAvailability {
    match translate_conversation_card(ConversationTranslationRequest {
        provider: params.provider,
        cli: params.cli,
        model: params.model,
        prompt: params.prompt,
    }) {
        Ok(_) => OpencodeTranslationAvailability {
            available: true,
            version: None,
            error: None,
        },
        Err(error) => OpencodeTranslationAvailability {
            available: false,
            version: None,
            error: Some(error),
        },
    }
}

pub(crate) fn list_conversation_translation_models(
    params: ConversationTranslationModelsRequest,
) -> ConversationTranslationModelsResult {
    let ConversationTranslationProvider::Cli = params.provider else {
        return ConversationTranslationModelsResult {
            models: Vec::new(),
            error: Some(
                "model listing is only available for CLI translation providers".to_string(),
            ),
        };
    };

    match params.cli {
        ConversationTranslationCli::Opencode => {
            match run_translation_cli_command(
                OPENCODE_COMMAND,
                &["models"],
                Duration::from_secs(20),
            ) {
                Ok(output) if output.status.success() => ConversationTranslationModelsResult {
                    models: parse_model_lines(&output.stdout),
                    error: None,
                },
                Ok(output) => ConversationTranslationModelsResult {
                    models: Vec::new(),
                    error: Some(command_failure_message("opencode models", &output)),
                },
                Err(error) => ConversationTranslationModelsResult {
                    models: Vec::new(),
                    error: Some(error),
                },
            }
        }
        ConversationTranslationCli::Gemini => ConversationTranslationModelsResult {
            models: Vec::new(),
            error: Some(
                "Gemini CLI does not expose a model listing command; enter a model manually"
                    .to_string(),
            ),
        },
    }
}

pub(crate) fn translate_conversation_card(
    params: ConversationTranslationRequest,
) -> AppResult<OpencodeTranslationResult> {
    validate_translation_prompt(&params.prompt)?;
    let model = normalize_model(&params.model)?;

    match params.provider {
        ConversationTranslationProvider::Cli => {
            translate_with_cli(params.cli, model.as_deref(), &params.prompt)
        }
        ConversationTranslationProvider::Google => {
            Err("Google Translate provider is reserved but not implemented yet".to_string())
        }
        ConversationTranslationProvider::Apple => {
            Err("Apple Translate provider is reserved but not implemented yet".to_string())
        }
    }
}

pub(crate) fn translate_conversation_card_with_opencode(
    params: OpencodeTranslationRequest,
) -> AppResult<OpencodeTranslationResult> {
    validate_translation_prompt(&params.prompt)?;
    let output = run_opencode_command(&["run", params.prompt.trim()], Duration::from_secs(180))?;
    if !output.status.success() {
        return Err(command_failure_message("opencode run", &output));
    }

    let translated_text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if translated_text.is_empty() {
        return Err("opencode returned an empty translation".to_string());
    }

    Ok(OpencodeTranslationResult { translated_text })
}

fn translate_with_cli(
    cli: ConversationTranslationCli,
    model: Option<&str>,
    prompt: &str,
) -> AppResult<OpencodeTranslationResult> {
    let prompt = prompt.trim();
    let (program, args) = match cli {
        ConversationTranslationCli::Opencode => {
            let mut args = vec!["run"];
            if let Some(model) = model {
                args.extend(["--model", model]);
            }
            args.push(prompt);
            (OPENCODE_COMMAND, args)
        }
        ConversationTranslationCli::Gemini => {
            let mut args = Vec::new();
            if let Some(model) = model {
                args.extend(["--model", model]);
            }
            args.extend(["--prompt", prompt]);
            (GEMINI_COMMAND, args)
        }
    };
    let output = run_translation_cli_command(program, &args, Duration::from_secs(180))?;
    if !output.status.success() {
        return Err(command_failure_message(
            &format!("{program} translation"),
            &output,
        ));
    }

    let translated_text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if translated_text.is_empty() {
        return Err(format!("{program} returned an empty translation"));
    }

    Ok(OpencodeTranslationResult { translated_text })
}

fn validate_translation_prompt(prompt: &str) -> AppResult<()> {
    let prompt = prompt.trim();
    if prompt.is_empty() {
        return Err("translation prompt is empty".to_string());
    }
    if prompt.len() > 200_000 {
        return Err("translation prompt is too large".to_string());
    }
    Ok(())
}

fn normalize_model(model: &str) -> AppResult<Option<String>> {
    let model = model.trim();
    if model.is_empty() {
        return Ok(None);
    }
    if model.len() > 120 || model.contains(['\n', '\r', '\0']) {
        return Err("translation model is invalid".to_string());
    }
    Ok(Some(model.to_string()))
}

fn run_opencode_command(args: &[&str], timeout: Duration) -> AppResult<Output> {
    let program = resolve_translation_cli_executable(OPENCODE_COMMAND)?;
    run_command_with_timeout(&program, args, timeout)
}

fn run_translation_cli_command(
    command_name: &str,
    args: &[&str],
    timeout: Duration,
) -> AppResult<Output> {
    let program = resolve_translation_cli_executable(command_name)?;
    run_command_with_timeout(&program, args, timeout)
}

fn resolve_translation_cli_executable(command_name: &str) -> AppResult<PathBuf> {
    let path_env = env::var_os("PATH");
    let login_shell_candidate = find_command_with_login_shell(command_name);
    let home_dir = dirs::home_dir();
    let search_candidates = translation_cli_search_candidates(command_name, home_dir.as_deref());
    resolve_translation_cli_executable_from_sources(
        command_name,
        path_env.as_deref(),
        login_shell_candidate,
        &search_candidates,
    )
}

fn resolve_translation_cli_executable_from_sources(
    command_name: &str,
    path_env: Option<&OsStr>,
    login_shell_candidate: Option<PathBuf>,
    search_candidates: &[PathBuf],
) -> AppResult<PathBuf> {
    if let Some(path) = find_program_on_path(command_name, path_env) {
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

    Err(format!("{command_name} was not found on this host. Install it and make `{command_name}` available on PATH or from a login shell."))
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
fn executable_name(command_name: &str) -> OsString {
    OsString::from(command_name)
}

#[cfg(windows)]
fn executable_name(command_name: &str) -> OsString {
    OsString::from(format!("{command_name}.exe"))
}

#[cfg(test)]
fn opencode_executable_name() -> OsString {
    executable_name(OPENCODE_COMMAND)
}

#[cfg(test)]
fn opencode_search_candidates(home_dir: Option<&Path>) -> Vec<PathBuf> {
    translation_cli_search_candidates(OPENCODE_COMMAND, home_dir)
}

fn translation_cli_search_candidates(command_name: &str, home_dir: Option<&Path>) -> Vec<PathBuf> {
    let executable = executable_name(command_name);
    let mut candidates = Vec::new();

    #[cfg(not(windows))]
    candidates.extend([
        Path::new("/opt/homebrew/bin").join(&executable),
        Path::new("/usr/local/bin").join(&executable),
        Path::new("/opt/local/bin").join(&executable),
    ]);

    if let Some(home_dir) = home_dir {
        candidates.extend([
            home_dir
                .join(format!(".{command_name}"))
                .join("bin")
                .join(&executable),
            home_dir.join(".local").join("bin").join(&executable),
            home_dir.join(".npm-global").join("bin").join(&executable),
            home_dir.join(".pnpm-global").join("bin").join(&executable),
            home_dir.join(".bun").join("bin").join(&executable),
            home_dir.join(".deno").join("bin").join(&executable),
            home_dir.join(".cargo").join("bin").join(&executable),
            home_dir.join(".volta").join("bin").join(&executable),
            home_dir.join("Library").join("pnpm").join(&executable),
        ]);
    }

    candidates
}

#[cfg(not(windows))]
fn find_command_with_login_shell(command_name: &str) -> Option<PathBuf> {
    let shell = login_shell()?;
    let script = format!("command -v {command_name}");
    let output = Command::new(shell)
        .args(["-lc", &script])
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
fn find_command_with_login_shell(_command_name: &str) -> Option<PathBuf> {
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

fn parse_model_lines(bytes: &[u8]) -> Vec<String> {
    String::from_utf8_lossy(bytes)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .filter(|line| !line.starts_with("opencode models"))
        .take(500)
        .map(str::to_string)
        .collect()
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

        let resolved = resolve_translation_cli_executable_from_sources(
            OPENCODE_COMMAND,
            Some(path_env.as_os_str()),
            None,
            &[],
        )
        .unwrap();

        assert_eq!(resolved, executable);
    }

    #[test]
    fn resolves_opencode_from_search_candidates_when_path_is_empty() {
        let dir = TempDir::new("assetiweave-opencode-candidate");
        let executable = dir.path().join(opencode_executable_name());
        write_executable(&executable);

        let resolved = resolve_translation_cli_executable_from_sources(
            OPENCODE_COMMAND,
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

        let resolved = resolve_translation_cli_executable_from_sources(
            OPENCODE_COMMAND,
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

    #[test]
    fn resolves_gemini_from_path_without_opencode_name() {
        let dir = TempDir::new("assetiweave-gemini-path");
        let executable = dir.path().join(executable_name(GEMINI_COMMAND));
        write_executable(&executable);
        let path_env = env::join_paths([dir.path()]).unwrap();

        let resolved = resolve_translation_cli_executable_from_sources(
            GEMINI_COMMAND,
            Some(path_env.as_os_str()),
            None,
            &[],
        )
        .unwrap();

        assert_eq!(resolved, executable);
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
