#[cfg(test)]
use crate::backend::host_process::{
    host_executable_name, host_executable_search_candidates,
    resolve_host_executable_from_sources as resolve_cli_executable_from_sources,
};
use crate::backend::{
    agents::types::AgentId,
    ai_execution::{
        execute_agent_blocking, legacy_gemini, AgentExecutionRuntime, AiCommandOptions,
        AiCommandOutput, AiExecutionCancellation, AiExecutionError, AiExecutionLimits,
        AiExecutionPurpose, AiExecutionRequest,
    },
    dto::AppResult,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
#[cfg(test)]
use std::{
    ffi::OsString,
    path::{Path, PathBuf},
};
use std::{sync::Arc, time::Duration};

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
    #[serde(default)]
    pub(crate) agent_id: Option<String>,
    #[serde(default = "default_translation_cli")]
    pub(crate) cli: ConversationTranslationCli,
    pub(crate) model: String,
    pub(crate) prompt: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct ConversationTranslationConnectionRequest {
    pub(crate) provider: ConversationTranslationProvider,
    #[serde(default)]
    pub(crate) agent_id: Option<String>,
    #[serde(default = "default_translation_cli")]
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

pub(crate) fn check_opencode_translation_availability(
    runtime: &dyn AgentExecutionRuntime,
) -> OpencodeTranslationAvailability {
    let availability = runtime.check_availability(&opencode_agent_id());
    OpencodeTranslationAvailability {
        available: availability.available,
        version: availability.version,
        error: availability.error.map(|error| error.to_string()),
    }
}

pub(crate) fn test_conversation_translation_connection(
    runtime: Arc<dyn AgentExecutionRuntime>,
    params: ConversationTranslationConnectionRequest,
) -> OpencodeTranslationAvailability {
    let result = match params.provider {
        ConversationTranslationProvider::Cli => {
            let model = normalize_model(&params.model);
            model.and_then(|model| {
                if let Some(agent_id) = params.agent_id.as_deref() {
                    execute_agent_translation(
                        runtime,
                        resolve_agent_id(Some(agent_id), params.cli)?,
                        params.prompt,
                        model,
                        AiExecutionPurpose::ConnectionTest,
                        connection_test_limits(),
                    )
                } else {
                    match params.cli {
                        ConversationTranslationCli::Opencode => execute_opencode_translation(
                            runtime,
                            params.prompt,
                            model,
                            AiExecutionPurpose::ConnectionTest,
                            connection_test_limits(),
                        ),
                        ConversationTranslationCli::Gemini => {
                            let translated_text = legacy_gemini::execute_translation(
                                model,
                                params.prompt,
                                translation_command_options(Duration::from_secs(30)),
                            )
                            .map_err(|error| {
                                translation_execution_error_message(
                                    "gemini",
                                    "gemini connection test",
                                    error,
                                )
                            })?;
                            Ok(OpencodeTranslationResult { translated_text })
                        }
                    }
                }
            })
        }
        provider => translate_conversation_card(
            runtime,
            ConversationTranslationRequest {
                provider,
                agent_id: params.agent_id,
                cli: params.cli,
                model: params.model,
                prompt: params.prompt,
            },
        ),
    };

    match result {
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
    runtime: &dyn AgentExecutionRuntime,
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
            match runtime.discover_models(&opencode_agent_id(), Duration::from_secs(20)) {
                Ok(output) => ConversationTranslationModelsResult {
                    models: parse_model_lines(&output),
                    error: None,
                },
                Err(error) => ConversationTranslationModelsResult {
                    models: Vec::new(),
                    error: Some(error.to_string()),
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
    runtime: Arc<dyn AgentExecutionRuntime>,
    params: ConversationTranslationRequest,
) -> AppResult<OpencodeTranslationResult> {
    validate_translation_prompt(&params.prompt)?;
    let model = normalize_model(&params.model)?;

    match params.provider {
        ConversationTranslationProvider::Cli => {
            if let Some(agent_id) = params.agent_id.as_deref() {
                execute_agent_translation(
                    runtime,
                    resolve_agent_id(Some(agent_id), params.cli)?,
                    params.prompt,
                    model,
                    AiExecutionPurpose::Translation,
                    AiExecutionLimits::default(),
                )
            } else {
                translate_with_cli(runtime, params.cli, model, params.prompt)
            }
        }
        ConversationTranslationProvider::Google => {
            Err("Google Translate provider is reserved but not implemented yet".to_string())
        }
        ConversationTranslationProvider::Apple => {
            Err("Apple Translate provider is reserved but not implemented yet".to_string())
        }
    }
}

pub(crate) fn prepare_opencode_agent_translation(
    params: ConversationTranslationRequest,
) -> AppResult<(AgentId, String, Option<String>)> {
    let ConversationTranslationProvider::Cli = params.provider else {
        return Err("AI execution tasks require a CLI translation provider".to_string());
    };
    validate_translation_prompt(&params.prompt)?;
    let model = normalize_model(&params.model)?;
    let agent_id = resolve_agent_id(params.agent_id.as_deref(), params.cli)?;
    Ok((agent_id, params.prompt.trim().to_string(), model))
}

pub(crate) fn translate_conversation_card_with_opencode(
    runtime: Arc<dyn AgentExecutionRuntime>,
    params: OpencodeTranslationRequest,
) -> AppResult<OpencodeTranslationResult> {
    validate_translation_prompt(&params.prompt)?;
    execute_opencode_translation(
        runtime,
        params.prompt,
        None,
        AiExecutionPurpose::Translation,
        AiExecutionLimits::default(),
    )
}

fn translate_with_cli(
    runtime: Arc<dyn AgentExecutionRuntime>,
    cli: ConversationTranslationCli,
    model: Option<String>,
    prompt: String,
) -> AppResult<OpencodeTranslationResult> {
    match cli {
        ConversationTranslationCli::Opencode => execute_opencode_translation(
            runtime,
            prompt,
            model,
            AiExecutionPurpose::Translation,
            AiExecutionLimits::default(),
        ),
        ConversationTranslationCli::Gemini => {
            let translated_text = legacy_gemini::execute_translation(
                model,
                prompt,
                translation_command_options(Duration::from_secs(180)),
            )
            .map_err(|error| {
                translation_execution_error_message("gemini", "gemini translation", error)
            })?;
            Ok(OpencodeTranslationResult { translated_text })
        }
    }
}

fn resolve_agent_id(agent_id: Option<&str>, cli: ConversationTranslationCli) -> AppResult<AgentId> {
    if let Some(agent_id) = agent_id.map(str::trim).filter(|value| !value.is_empty()) {
        return AgentId::parse(agent_id).map_err(|error| error.to_string());
    }
    let legacy_id = match cli {
        ConversationTranslationCli::Opencode => "opencode",
        ConversationTranslationCli::Gemini => "gemini",
    };
    AgentId::parse(legacy_id).map_err(|error| error.to_string())
}

fn default_translation_cli() -> ConversationTranslationCli {
    ConversationTranslationCli::Opencode
}

fn execute_agent_translation(
    runtime: Arc<dyn AgentExecutionRuntime>,
    agent_id: AgentId,
    prompt: String,
    model: Option<String>,
    purpose: AiExecutionPurpose,
    limits: AiExecutionLimits,
) -> AppResult<OpencodeTranslationResult> {
    validate_translation_prompt(&prompt)?;
    let request = AiExecutionRequest {
        execution_id: uuid::Uuid::new_v4().to_string(),
        agent_id,
        purpose,
        prompt: prompt.trim().to_string(),
        model,
        limits,
        cancellation: AiExecutionCancellation::default(),
        progress: None,
    };
    request.validate().map_err(agent_execution_error_message)?;
    let result = execute_agent_blocking(runtime, request).map_err(agent_execution_error_message)?;
    Ok(OpencodeTranslationResult {
        translated_text: result.text,
    })
}

fn execute_opencode_translation(
    runtime: Arc<dyn AgentExecutionRuntime>,
    prompt: String,
    model: Option<String>,
    purpose: AiExecutionPurpose,
    limits: AiExecutionLimits,
) -> AppResult<OpencodeTranslationResult> {
    validate_translation_prompt(&prompt)?;
    let prompt = prompt.trim().to_string();
    execute_agent_translation(runtime, opencode_agent_id(), prompt, model, purpose, limits)
}

fn opencode_agent_id() -> AgentId {
    AgentId::parse("opencode").expect("builtin OpenCode agent id must be valid")
}

fn connection_test_limits() -> AiExecutionLimits {
    AiExecutionLimits {
        total_timeout: Duration::from_secs(30),
        ..AiExecutionLimits::default()
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

fn agent_execution_error_message(error: AiExecutionError) -> String {
    let view = error.to_view();
    format!("{}: {}", view.code, view.message)
}

#[cfg(test)]
fn opencode_executable_name() -> OsString {
    host_executable_name(OPENCODE_COMMAND)
}

#[cfg(test)]
fn opencode_search_candidates(home_dir: Option<&Path>) -> Vec<PathBuf> {
    host_executable_search_candidates(OPENCODE_COMMAND, home_dir)
}

fn first_nonempty_line(bytes: &[u8]) -> Option<String> {
    String::from_utf8_lossy(bytes)
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(str::to_string)
}

fn parse_model_lines(bytes: &[u8]) -> Vec<String> {
    let mut models = String::from_utf8_lossy(bytes)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .filter(|line| !line.starts_with("opencode models"))
        .map(str::to_string)
        .collect::<Vec<_>>();
    models.sort();
    models.dedup();
    models.truncate(500);
    models
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
    use crate::backend::{
        agents::{
            registry::{AgentAvailability, AgentProbeError},
            types::AgentProtocol,
        },
        ai_execution::{executor::BackendFuture, AiExecutionResult},
    };
    use std::{
        env, fs,
        path::Path,
        sync::{Arc, Mutex},
    };

    struct FakeRuntime {
        requests: Mutex<Vec<AiExecutionRequest>>,
        result_text: String,
    }

    impl FakeRuntime {
        fn new(result_text: &str) -> Arc<Self> {
            Arc::new(Self {
                requests: Mutex::new(Vec::new()),
                result_text: result_text.to_string(),
            })
        }
    }

    impl AgentExecutionRuntime for FakeRuntime {
        fn execute<'a>(&'a self, request: AiExecutionRequest) -> BackendFuture<'a> {
            Box::pin(async move {
                self.requests.lock().unwrap().push(request.clone());
                Ok(AiExecutionResult {
                    text: self.result_text.clone(),
                    agent_id: request.agent_id,
                    protocol: AgentProtocol::Acp,
                    requested_model: request.model,
                    elapsed_ms: 1,
                })
            })
        }

        fn check_availability(&self, agent_id: &AgentId) -> AgentAvailability {
            assert_eq!(agent_id.as_str(), "opencode");
            AgentAvailability {
                available: true,
                installed: true,
                version: Some("opencode-test 1.0".to_string()),
                error: None,
            }
        }

        fn discover_models(
            &self,
            agent_id: &AgentId,
            timeout: Duration,
        ) -> Result<Vec<u8>, AgentProbeError> {
            assert_eq!(agent_id.as_str(), "opencode");
            assert_eq!(timeout, Duration::from_secs(20));
            Ok(b"model/z\nmodel/a\nmodel/z\n".to_vec())
        }
    }

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
    fn tr_01_02_03_opencode_translation_maps_to_agent_runtime_without_legacy_run() {
        let runtime = FakeRuntime::new("译文");

        let result = translate_conversation_card(
            runtime.clone(),
            ConversationTranslationRequest {
                agent_id: None,
                provider: ConversationTranslationProvider::Cli,
                cli: ConversationTranslationCli::Opencode,
                model: " model/a ".to_string(),
                prompt: "  translate this  ".to_string(),
            },
        )
        .unwrap();

        assert_eq!(result.translated_text, "译文");
        let requests = runtime.requests.lock().unwrap();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].agent_id.as_str(), "opencode");
        assert_eq!(requests[0].purpose, AiExecutionPurpose::Translation);
        assert_eq!(requests[0].model.as_deref(), Some("model/a"));
        assert_eq!(requests[0].prompt, "translate this");
    }

    #[test]
    fn tr_03_compatibility_opencode_request_maps_runtime_text() {
        let runtime = FakeRuntime::new("compat result");

        let result = translate_conversation_card_with_opencode(
            runtime.clone(),
            OpencodeTranslationRequest {
                prompt: "translate".to_string(),
            },
        )
        .unwrap();

        assert_eq!(result.translated_text, "compat result");
        let requests = runtime.requests.lock().unwrap();
        assert_eq!(requests[0].agent_id.as_str(), "opencode");
        assert_eq!(requests[0].model, None);
    }

    #[test]
    fn tr_04_05_invalid_prompt_and_model_fail_before_runtime() {
        let runtime = FakeRuntime::new("unused");

        let oversized = translate_conversation_card(
            runtime.clone(),
            ConversationTranslationRequest {
                agent_id: None,
                provider: ConversationTranslationProvider::Cli,
                cli: ConversationTranslationCli::Opencode,
                model: String::new(),
                prompt: "x".repeat(200_001),
            },
        );
        let invalid_model = translate_conversation_card(
            runtime.clone(),
            ConversationTranslationRequest {
                agent_id: None,
                provider: ConversationTranslationProvider::Cli,
                cli: ConversationTranslationCli::Opencode,
                model: "bad\nmodel".to_string(),
                prompt: "translate".to_string(),
            },
        );

        assert_eq!(oversized.unwrap_err(), "translation prompt is too large");
        assert_eq!(invalid_model.unwrap_err(), "translation model is invalid");
        assert!(runtime.requests.lock().unwrap().is_empty());
    }

    #[test]
    fn tr_06_connection_test_uses_agent_runtime_and_shorter_limit() {
        let runtime = FakeRuntime::new("OK");

        let availability = test_conversation_translation_connection(
            runtime.clone(),
            ConversationTranslationConnectionRequest {
                agent_id: None,
                provider: ConversationTranslationProvider::Cli,
                cli: ConversationTranslationCli::Opencode,
                model: "model/a".to_string(),
                prompt: "Reply with OK only.".to_string(),
            },
        );

        assert!(availability.available);
        let requests = runtime.requests.lock().unwrap();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].purpose, AiExecutionPurpose::ConnectionTest);
        assert_eq!(requests[0].limits.total_timeout, Duration::from_secs(30));
    }

    #[test]
    fn tr_07_availability_maps_the_runtime_registry_probe() {
        let runtime = FakeRuntime::new("unused");

        let availability = check_opencode_translation_availability(runtime.as_ref());

        assert!(availability.available);
        assert_eq!(availability.version.as_deref(), Some("opencode-test 1.0"));
        assert_eq!(availability.error, None);
    }

    #[test]
    fn tr_08_model_list_uses_runtime_discovery_and_returns_stable_unique_models() {
        let runtime = FakeRuntime::new("unused");

        let result = list_conversation_translation_models(
            runtime.as_ref(),
            ConversationTranslationModelsRequest {
                provider: ConversationTranslationProvider::Cli,
                cli: ConversationTranslationCli::Opencode,
            },
        );

        assert_eq!(result.models, ["model/a", "model/z"]);
        assert_eq!(result.error, None);

        let over_limit = (0..501)
            .rev()
            .map(|index| format!("model/{index:03}\n"))
            .collect::<String>();
        let parsed = parse_model_lines(over_limit.as_bytes());
        assert_eq!(parsed.len(), 500);
        assert_eq!(parsed.first().map(String::as_str), Some("model/000"));
        assert_eq!(parsed.last().map(String::as_str), Some("model/499"));
    }

    #[test]
    fn tr_10_reserved_providers_keep_existing_errors_without_execution() {
        let runtime = FakeRuntime::new("unused");

        let google = translate_conversation_card(
            runtime.clone(),
            ConversationTranslationRequest {
                agent_id: None,
                provider: ConversationTranslationProvider::Google,
                cli: ConversationTranslationCli::Opencode,
                model: String::new(),
                prompt: "translate".to_string(),
            },
        )
        .unwrap_err();
        let apple = translate_conversation_card(
            runtime.clone(),
            ConversationTranslationRequest {
                agent_id: None,
                provider: ConversationTranslationProvider::Apple,
                cli: ConversationTranslationCli::Opencode,
                model: String::new(),
                prompt: "translate".to_string(),
            },
        )
        .unwrap_err();

        assert_eq!(
            google,
            "Google Translate provider is reserved but not implemented yet"
        );
        assert_eq!(
            apple,
            "Apple Translate provider is reserved but not implemented yet"
        );
        assert!(runtime.requests.lock().unwrap().is_empty());
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
        let executable = dir.path().join(host_executable_name(GEMINI_COMMAND));
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
