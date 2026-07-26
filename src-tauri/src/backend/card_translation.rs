#[cfg(test)]
use crate::backend::ai_execution::{
    cli_search_candidates, executable_name, resolve_cli_executable_from_sources,
};
use crate::backend::{
    ai_execution::{
        execute_structured_text, run_cli_command, AiCliRuntime, AiCommandOptions, AiCommandOutput,
        AiExecutionError, AiStructuredTextRequest,
    },
    dto::AppResult,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::time::Duration;
#[cfg(test)]
use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};

#[cfg(test)]
const OPENCODE_COMMAND: &str = "opencode";
#[cfg(test)]
const GEMINI_COMMAND: &str = "gemini";
const TRANSLATION_STDOUT_CAP: usize = 1024 * 1024;
const TRANSLATION_STDERR_CAP: usize = 256 * 1024;

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

#[derive(Clone, Copy, Debug, Deserialize, JsonSchema)]
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
    match run_translation_cli_command(
        AiCliRuntime::Opencode,
        &["--version"],
        Duration::from_secs(8),
    ) {
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
                AiCliRuntime::Opencode,
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
            translate_with_cli(params.cli, model, params.prompt)
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
    let translated_text =
        execute_translation_text(AiCliRuntime::Opencode, None, params.prompt, "opencode run")?;

    Ok(OpencodeTranslationResult { translated_text })
}

fn translate_with_cli(
    cli: ConversationTranslationCli,
    model: Option<String>,
    prompt: String,
) -> AppResult<OpencodeTranslationResult> {
    let runtime = translation_runtime(cli);
    let program = runtime.command_name();
    let translated_text =
        execute_translation_text(runtime, model, prompt, &format!("{program} translation"))?;

    Ok(OpencodeTranslationResult { translated_text })
}

fn execute_translation_text(
    runtime: AiCliRuntime,
    model: Option<String>,
    prompt: String,
    failure_label: &str,
) -> AppResult<String> {
    let program = runtime.command_name();
    let result = execute_structured_text(AiStructuredTextRequest {
        runtime,
        model,
        prompt,
        options: translation_command_options(Duration::from_secs(180)),
    })
    .map_err(|error| translation_execution_error_message(program, failure_label, error))?;
    Ok(result.text)
}

fn translation_runtime(cli: ConversationTranslationCli) -> AiCliRuntime {
    match cli {
        ConversationTranslationCli::Opencode => AiCliRuntime::Opencode,
        ConversationTranslationCli::Gemini => AiCliRuntime::Gemini,
    }
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

fn run_translation_cli_command(
    runtime: AiCliRuntime,
    args: &[&str],
    timeout: Duration,
) -> AppResult<AiCommandOutput> {
    let args = args
        .iter()
        .map(|arg| (*arg).to_string())
        .collect::<Vec<_>>();
    let output = run_cli_command(runtime, &args, translation_command_options(timeout))
        .map_err(|error| error.to_string())?;
    if output.status.success() && output.stdout_truncated {
        return Err(format!(
            "{} exceeded the configured output limit",
            output.program.display()
        ));
    }
    Ok(output)
}

fn translation_command_options(timeout: Duration) -> AiCommandOptions {
    AiCommandOptions::new(timeout, TRANSLATION_STDOUT_CAP, TRANSLATION_STDERR_CAP)
}

fn translation_execution_error_message(
    program: &str,
    failure_label: &str,
    error: AiExecutionError,
) -> String {
    match error {
        AiExecutionError::CommandFailed(output) => command_failure_message(failure_label, &output),
        AiExecutionError::EmptyOutput { .. } => {
            format!("{program} returned an empty translation")
        }
        other => other.to_string(),
    }
}

#[cfg(test)]
fn opencode_executable_name() -> OsString {
    executable_name(OPENCODE_COMMAND)
}

#[cfg(test)]
fn opencode_search_candidates(home_dir: Option<&Path>) -> Vec<PathBuf> {
    cli_search_candidates(OPENCODE_COMMAND, home_dir)
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

fn command_failure_message(command_name: &str, output: &AiCommandOutput) -> String {
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

        let resolved = resolve_cli_executable_from_sources(
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

        let resolved = resolve_cli_executable_from_sources(
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

        let resolved = resolve_cli_executable_from_sources(
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

        let resolved = resolve_cli_executable_from_sources(
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
