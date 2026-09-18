use super::*;
use crate::backend::{
    agents::{
        registry::{AgentAvailability, AgentProbeError},
        types::AgentProtocol,
    },
    ai_execution::{executor::BackendFuture, AiExecutionResult},
    host_process::{
        host_executable_name, host_executable_search_candidates,
        resolve_host_executable_from_sources as resolve_cli_executable_from_sources,
    },
};
use std::{
    env,
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

const OPENCODE_COMMAND: &str = "opencode";
const GEMINI_COMMAND: &str = "gemini";

fn check_opencode_translation_availability(
    runtime: &dyn AgentExecutionRuntime,
) -> OpencodeTranslationAvailability {
    let settings = crate::backend::runtime::current_process_runtime()
        .map(|r| r.app_settings_value())
        .unwrap_or_else(|| {
            crate::backend::app_settings::canonicalize_settings(serde_json::json!({}))
                .unwrap_or_default()
        });
    check_opencode_translation_availability_with_settings(runtime, &settings)
}

fn opencode_executable_name() -> OsString {
    host_executable_name(OPENCODE_COMMAND)
}

fn opencode_search_candidates(home_dir: Option<&Path>) -> Vec<PathBuf> {
    host_executable_search_candidates(OPENCODE_COMMAND, home_dir)
}

#[test]
fn ai_model_selection_error_keeps_its_structured_code() {
    let error = app_error_from_ai(AiExecutionError::ModelSelectionFailed {
        detail: Some("fixture model is unavailable".to_string()),
    });
    let view = error.view();

    assert_eq!(view.code, "model_selection_failed");
    assert!(!view.retryable);
    assert!(view.message.contains("fixture model is unavailable"));
}

#[test]
fn ai_protocol_detail_keeps_retryability_and_detail() {
    let error = app_error_from_ai(AiExecutionError::ProtocolDetail {
        operation: "initialize",
        detail: "fixture handshake failed".to_string(),
    });
    let view = error.view();

    assert_eq!(view.code, "protocol_failed");
    assert!(view.retryable);
    assert!(view.message.contains("fixture handshake failed"));
}

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
                persistent_binding: None,
                replay_text: None,
                session_cleanup: crate::backend::ai_execution::SessionCleanupStatus::Deleted,
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

#[tokio::test]
async fn tr_01_02_03_opencode_translation_maps_to_agent_runtime_without_legacy_run() {
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
    .await
    .unwrap();

    assert_eq!(result.translated_text, "译文");
    let requests = runtime.requests.lock().unwrap();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].agent_id.as_str(), "opencode");
    assert_eq!(requests[0].purpose, AiExecutionPurpose::Translation);
    assert_eq!(requests[0].model.as_deref(), Some("model/a"));
    assert_eq!(requests[0].prompt, "translate this");
}

#[tokio::test]
async fn tr_01_gemini_translation_maps_to_agent_runtime_without_special_process_logic() {
    let runtime = FakeRuntime::new("Gemini 译文");

    let result = translate_conversation_card(
        runtime.clone(),
        ConversationTranslationRequest {
            agent_id: None,
            provider: ConversationTranslationProvider::Cli,
            cli: ConversationTranslationCli::Gemini,
            model: "gemini-2.5-pro".to_string(),
            prompt: "translate with Gemini".to_string(),
        },
    )
    .await
    .unwrap();

    assert_eq!(result.translated_text, "Gemini 译文");
    let requests = runtime.requests.lock().unwrap();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].agent_id.as_str(), "gemini");
    assert_eq!(requests[0].purpose, AiExecutionPurpose::Translation);
    assert_eq!(requests[0].model.as_deref(), Some("gemini-2.5-pro"));
}

#[tokio::test]
async fn prompt_optimization_has_a_distinct_execution_purpose_and_result_contract() {
    let runtime = FakeRuntime::new("优化后的提示词");

    let result = optimize_prompt(
        runtime.clone(),
        PromptOptimizationRequest {
            agent_id: Some("opencode".to_string()),
            provider: ConversationTranslationProvider::Cli,
            cli: ConversationTranslationCli::Opencode,
            model: "model/a".to_string(),
            prompt: "optimize this prompt".to_string(),
        },
    )
    .await
    .unwrap();

    assert_eq!(result.optimized_text, "优化后的提示词");
    let requests = runtime.requests.lock().unwrap();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].purpose, AiExecutionPurpose::PromptOptimization);
    assert_eq!(requests[0].prompt, "optimize this prompt");
}

#[tokio::test]
async fn tr_01_gemini_connection_test_uses_the_same_agent_runtime() {
    let runtime = FakeRuntime::new("connection ok");

    let availability = test_conversation_translation_connection(
        runtime.clone(),
        ConversationTranslationConnectionRequest {
            agent_id: None,
            provider: ConversationTranslationProvider::Cli,
            cli: ConversationTranslationCli::Gemini,
            model: "gemini-2.5-pro".to_string(),
            prompt: "connection test".to_string(),
        },
    )
    .await;

    assert!(availability.available);
    let requests = runtime.requests.lock().unwrap();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].agent_id.as_str(), "gemini");
    assert_eq!(requests[0].purpose, AiExecutionPurpose::ConnectionTest);
}

#[tokio::test]
async fn tr_03_compatibility_opencode_request_maps_runtime_text() {
    let runtime = FakeRuntime::new("compat result");

    let result = translate_conversation_card_with_opencode(
        runtime.clone(),
        OpencodeTranslationRequest {
            prompt: "translate".to_string(),
        },
    )
    .await
    .unwrap();

    assert_eq!(result.translated_text, "compat result");
    let requests = runtime.requests.lock().unwrap();
    assert_eq!(requests[0].agent_id.as_str(), "opencode");
    assert_eq!(requests[0].model, None);
}

#[tokio::test]
async fn tr_04_05_invalid_prompt_and_model_fail_before_runtime() {
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
    )
    .await;
    let invalid_model = translate_conversation_card(
        runtime.clone(),
        ConversationTranslationRequest {
            agent_id: None,
            provider: ConversationTranslationProvider::Cli,
            cli: ConversationTranslationCli::Opencode,
            model: "bad\nmodel".to_string(),
            prompt: "translate".to_string(),
        },
    )
    .await;

    assert!(matches!(
        oversized.unwrap_err(),
        AppError::Validation(message) if message == "translation prompt is too large"
    ));
    assert!(matches!(
        invalid_model.unwrap_err(),
        AppError::Validation(message) if message == "translation model is invalid"
    ));
    assert!(runtime.requests.lock().unwrap().is_empty());
}

#[tokio::test]
async fn tr_06_connection_test_uses_agent_runtime_and_shorter_limit() {
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
    )
    .await;

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

#[tokio::test]
async fn tr_10_reserved_providers_keep_existing_errors_without_execution() {
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
    .await
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
    .await
    .unwrap_err();

    assert_eq!(
        google.to_string(),
        "Google Translate provider is reserved but not implemented yet"
    );
    assert_eq!(
        apple.to_string(),
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

    assert!(candidates.contains(&Path::new("/opt/homebrew/bin").join(opencode_executable_name())));
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

    let resolved =
        resolve_cli_executable_from_sources(GEMINI_COMMAND, Some(path_env.as_os_str()), None, &[])
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
