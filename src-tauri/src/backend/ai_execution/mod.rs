pub(crate) mod backends;
pub(crate) mod composition;
mod error;
pub(crate) mod executor;
pub(crate) mod legacy_gemini;
mod types;

pub(crate) use error::{AiExecutionError, AiExecutionErrorView};
pub(crate) use executor::AgentExecutionRuntime;
pub(crate) use types::{
    AiExecutionCancellation, AiExecutionLimits, AiExecutionPhase, AiExecutionProgressSink,
    AiExecutionPurpose, AiExecutionRequest, AiExecutionResult,
};

use crate::backend::agents::types::{
    AgentConnectionCheckMode, AgentConnectionResult, AgentId, AgentModelsResult,
};
use crate::backend::host_process::{
    resolve_host_executable, run_command_with_control, HostProcessControl, HostProcessError,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{
    ffi::OsString,
    path::{Path, PathBuf},
    process::{Command, ExitStatus},
    sync::Arc,
    time::Duration,
};

#[cfg(test)]
pub(crate) fn agent_runtime_manager(
    db_path: &Path,
) -> Result<Arc<crate::backend::agent_market::AgentRuntimeManager>, AiExecutionError> {
    let db = crate::backend::store::Database::open_initialized(db_path).map_err(|_| {
        AiExecutionError::Protocol {
            operation: "runtime_database_initialize",
        }
    })?;
    let pool = db.pool().clone();
    let context = db
        .block_on(
            async move { crate::backend::store::load_local_request_context_sqlx(&pool).await },
        )
        .map_err(|_| AiExecutionError::Protocol {
            operation: "runtime_context_initialize",
        })?;
    let workspace_root = db_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("agent-executions");
    let manager = Arc::new(crate::backend::agent_market::AgentRuntimeManager::new(
        db.pool().clone(),
        workspace_root,
    ));
    let runtime_root = crate::backend::agent_market::default_runtime_root().map_err(|_| {
        AiExecutionError::Protocol {
            operation: "runtime_root_initialize",
        }
    })?;
    db.block_on(manager.recover_startup(&context.tenant.id, &runtime_root))
        .map_err(|_| AiExecutionError::Protocol {
            operation: "runtime_registry_reload",
        })?;
    Ok(manager)
}

#[deprecated(note = "use composition::resolve_agent_for with an explicit ActionId")]
pub(crate) fn configured_agent_capability(
    service_id: &str,
) -> Result<(AgentId, Option<String>), String> {
    let action = match service_id {
        "memory" => composition::ActionId::new("memory.extraction"),
        "cardTranslation" | "card_translation" => composition::ActionId::new("translation"),
        "promptOptimization" | "prompt_optimization" => {
            composition::ActionId::new("prompt_optimization")
        }
        other => composition::ActionId::new(other),
    };
    composition::resolve_agent_for(&action).map_err(|error| error.to_string())
}

/// Runs the async Agent runtime from synchronous application/Engine seams.
///
/// The dedicated thread is intentional: callers may already be inside a Tokio
/// runtime, where nesting `Runtime::block_on` would panic. Desktop background
/// tasks should call `AgentExecutionRuntime::execute` directly instead.
pub(crate) fn execute_agent_blocking(
    runtime: Arc<dyn AgentExecutionRuntime>,
    request: AiExecutionRequest,
) -> Result<AiExecutionResult, AiExecutionError> {
    std::thread::spawn(move || {
        let runtime_driver = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|_| AiExecutionError::Protocol {
                operation: "blocking_runtime_initialize",
            })?;
        runtime_driver.block_on(runtime.execute(request))
    })
    .join()
    .map_err(|_| AiExecutionError::Protocol {
        operation: "blocking_runtime_join",
    })?
}

pub(crate) fn check_agent_connection_blocking(
    runtime: Arc<dyn AgentExecutionRuntime>,
    agent_id: AgentId,
    mode: AgentConnectionCheckMode,
) -> AgentConnectionResult {
    if matches!(mode, AgentConnectionCheckMode::Installation) {
        return runtime.check_agent_installation(&agent_id);
    }

    let thread_runtime = runtime.clone();
    let fallback_agent_id = agent_id.clone();
    std::thread::spawn(move || {
        let runtime_driver = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(runtime_driver) => runtime_driver,
            Err(_) => {
                return connection_probe_failure(
                    &agent_id,
                    "connection_runtime_initialize",
                    "The Agent connection probe runtime could not be initialized.",
                )
            }
        };
        runtime_driver.block_on(thread_runtime.check_agent_connection(&agent_id))
    })
    .join()
    .unwrap_or_else(|_| {
        connection_probe_failure(
            &fallback_agent_id,
            "connection_runtime_join",
            "The Agent connection probe did not complete.",
        )
    })
}

pub(crate) fn discover_agent_models_blocking(
    runtime: Arc<dyn AgentExecutionRuntime>,
    agent_id: AgentId,
) -> AgentModelsResult {
    let fallback_agent_id = agent_id.clone();
    std::thread::spawn(move || {
        let runtime_driver = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(runtime_driver) => runtime_driver,
            Err(_) => {
                return unavailable_models_result(
                    &agent_id,
                    "model_discovery_runtime_initialize",
                    "The Agent model discovery runtime could not be initialized.",
                )
            }
        };
        runtime_driver.block_on(runtime.discover_agent_models(&agent_id))
    })
    .join()
    .unwrap_or_else(|_| {
        unavailable_models_result(
            &fallback_agent_id,
            "model_discovery_runtime_join",
            "The Agent model discovery did not complete.",
        )
    })
}

fn connection_probe_failure(
    agent_id: &AgentId,
    error_code: &str,
    error: &str,
) -> AgentConnectionResult {
    AgentConnectionResult {
        agent_id: agent_id.to_string(),
        available: false,
        installed: false,
        connected: false,
        version: None,
        connection_method: None,
        error_code: Some(error_code.to_string()),
        error: Some(error.to_string()),
        installation_status: None,
        runtime_status: None,
        protocol_status: None,
        execution_ready: false,
        health_stale: false,
    }
}

fn unavailable_models_result(
    agent_id: &AgentId,
    error_code: &str,
    error: &str,
) -> AgentModelsResult {
    AgentModelsResult {
        agent_id: agent_id.to_string(),
        available: false,
        models: Vec::new(),
        current_model_id: None,
        error_code: Some(error_code.to_string()),
        error: Some(error.to_string()),
    }
}

const MAX_PROMPT_BYTES: usize = 1_000_000;

#[derive(Clone, Copy, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum AiCliRuntime {
    Opencode,
    Gemini,
}

impl AiCliRuntime {
    pub(crate) fn command_name(self) -> &'static str {
        match self {
            Self::Opencode => "opencode",
            Self::Gemini => "gemini",
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) struct AiCommandOptions {
    pub(crate) timeout: Duration,
    pub(crate) stdout_cap: usize,
    pub(crate) stderr_cap: usize,
    pub(crate) current_dir: Option<PathBuf>,
    pub(crate) environment: Vec<(OsString, OsString)>,
    pub(crate) cancellation: Option<AiExecutionCancellation>,
}

impl AiCommandOptions {
    pub(crate) fn new(timeout: Duration, stdout_cap: usize, stderr_cap: usize) -> Self {
        Self {
            timeout,
            stdout_cap,
            stderr_cap,
            current_dir: None,
            environment: Vec::new(),
            cancellation: None,
        }
    }
}

#[derive(Debug)]
pub(crate) struct AiCommandOutput {
    pub(crate) program: PathBuf,
    pub(crate) status: ExitStatus,
    pub(crate) stdout: Vec<u8>,
    pub(crate) stderr: Vec<u8>,
    pub(crate) stdout_truncated: bool,
    pub(crate) stderr_truncated: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct AiStructuredTextRequest {
    pub(crate) runtime: AiCliRuntime,
    pub(crate) model: Option<String>,
    pub(crate) prompt: String,
    pub(crate) options: AiCommandOptions,
}

#[derive(Debug)]
pub(crate) struct AiStructuredTextResult {
    pub(crate) text: String,
    pub(crate) stderr: String,
    pub(crate) stderr_truncated: bool,
}

pub(crate) fn execute_structured_text(
    request: AiStructuredTextRequest,
) -> Result<AiStructuredTextResult, AiExecutionError> {
    let prompt = normalize_prompt(&request.prompt)?;
    let model = normalize_model(request.model.as_deref())?;
    let args = structured_text_args(request.runtime, model.as_deref(), &prompt);
    let output = run_cli_command(request.runtime, &args, request.options)?;
    normalize_structured_text_output(output)
}

pub(crate) fn run_cli_command(
    runtime: AiCliRuntime,
    args: &[String],
    options: AiCommandOptions,
) -> Result<AiCommandOutput, AiExecutionError> {
    let program = resolve_cli_executable(runtime)?;
    run_cli_command_at_path(&program, args, options)
}

fn run_cli_command_at_path(
    program: &Path,
    args: &[String],
    options: AiCommandOptions,
) -> Result<AiCommandOutput, AiExecutionError> {
    let mut command = Command::new(program);
    command.args(args);
    if let Some(current_dir) = options.current_dir.as_deref() {
        command.current_dir(current_dir);
    }
    command.envs(options.environment.iter().map(|(key, value)| (key, value)));

    let output = run_command_with_control(
        &mut command,
        HostProcessControl {
            timeout: options.timeout,
            stdout_cap: options.stdout_cap,
            stderr_cap: options.stderr_cap,
            cancellation: options
                .cancellation
                .as_ref()
                .map(AiExecutionCancellation::flag),
        },
    )
    .map_err(|error| normalize_host_process_error(program, options.timeout, error))?;

    Ok(AiCommandOutput {
        program: program.to_path_buf(),
        status: output.status,
        stdout: output.stdout,
        stderr: output.stderr,
        stdout_truncated: output.stdout_truncated,
        stderr_truncated: output.stderr_truncated,
    })
}

fn normalize_structured_text_output(
    output: AiCommandOutput,
) -> Result<AiStructuredTextResult, AiExecutionError> {
    if !output.status.success() {
        return Err(AiExecutionError::CommandFailed(output));
    }
    if output.stdout_truncated {
        return Err(AiExecutionError::OutputLimit {
            limit: output.stdout.len(),
            legacy_output: Some(Box::new(output)),
        });
    }

    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if text.is_empty() {
        return Err(AiExecutionError::EmptyOutput {
            program: Some(output.program),
        });
    }

    Ok(AiStructuredTextResult {
        text,
        stderr: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        stderr_truncated: output.stderr_truncated,
    })
}

fn normalize_host_process_error(
    program: &Path,
    timeout: Duration,
    error: HostProcessError,
) -> AiExecutionError {
    match error {
        HostProcessError::Spawn(message) => AiExecutionError::Spawn {
            program: program.to_path_buf(),
            message,
        },
        HostProcessError::Output(message) => AiExecutionError::Output {
            program: program.to_path_buf(),
            message,
        },
        HostProcessError::Timeout {
            stdout,
            stderr,
            stdout_truncated,
            stderr_truncated,
        } => AiExecutionError::Timeout {
            program: program.to_path_buf(),
            timeout,
            stdout,
            stderr,
            stdout_truncated,
            stderr_truncated,
        },
        HostProcessError::Cancelled {
            stdout,
            stderr,
            stdout_truncated,
            stderr_truncated,
        } => AiExecutionError::Cancelled {
            program: program.to_path_buf(),
            stdout,
            stderr,
            stdout_truncated,
            stderr_truncated,
        },
    }
}

fn normalize_prompt(prompt: &str) -> Result<String, AiExecutionError> {
    let prompt = prompt.trim();
    if prompt.is_empty() {
        return Err(AiExecutionError::InvalidPrompt(
            "AI prompt is empty".to_string(),
        ));
    }
    if prompt.len() > MAX_PROMPT_BYTES {
        return Err(AiExecutionError::InvalidPrompt(format!(
            "AI prompt exceeds the {MAX_PROMPT_BYTES}-byte limit"
        )));
    }
    Ok(prompt.to_string())
}

fn normalize_model(model: Option<&str>) -> Result<Option<String>, AiExecutionError> {
    let Some(model) = model.map(str::trim).filter(|model| !model.is_empty()) else {
        return Ok(None);
    };
    if model.len() > 120 || model.contains(['\n', '\r', '\0']) {
        return Err(AiExecutionError::InvalidModel(
            "AI model is invalid".to_string(),
        ));
    }
    Ok(Some(model.to_string()))
}

fn structured_text_args(runtime: AiCliRuntime, model: Option<&str>, prompt: &str) -> Vec<String> {
    match runtime {
        AiCliRuntime::Opencode => {
            let mut args = vec!["run".to_string()];
            if let Some(model) = model {
                args.extend(["--model".to_string(), model.to_string()]);
            }
            args.push(prompt.to_string());
            args
        }
        AiCliRuntime::Gemini => {
            let mut args = Vec::new();
            if let Some(model) = model {
                args.extend(["--model".to_string(), model.to_string()]);
            }
            args.extend(["--prompt".to_string(), prompt.to_string()]);
            args
        }
    }
}

pub(crate) fn resolve_cli_executable(runtime: AiCliRuntime) -> Result<PathBuf, AiExecutionError> {
    let command_name = runtime.command_name();
    resolve_host_executable(command_name).ok_or_else(|| AiExecutionError::RuntimeUnavailable {
        command_name: command_name.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        env,
        io::{self, Write},
        time::Instant,
    };

    #[test]
    fn process_fixture() {
        match env::var("ASSETIWEAVE_AI_EXECUTION_FIXTURE").as_deref() {
            Ok("large-output") => {
                io::stdout().write_all(&vec![b'x'; 256 * 1024]).unwrap();
                io::stderr().write_all(&vec![b'y'; 256 * 1024]).unwrap();
            }
            Ok("child-tree") => {
                let mut command = Command::new(env::current_exe().expect("resolve test binary"));
                let mut child = command
                    .args([
                        "--exact",
                        "backend::ai_execution::tests::process_fixture",
                        "--nocapture",
                    ])
                    .env("ASSETIWEAVE_AI_EXECUTION_FIXTURE", "grandchild")
                    .spawn()
                    .expect("spawn grandchild fixture");
                std::thread::sleep(Duration::from_secs(5));
                let _ = child.wait();
            }
            Ok("grandchild") => std::thread::sleep(Duration::from_secs(5)),
            Ok("timeout") => std::thread::sleep(Duration::from_secs(5)),
            _ => {}
        }
    }

    #[test]
    fn drains_large_stdout_and_stderr_without_deadlock() {
        let output = run_fixture("large-output", None, 64 * 1024, 32 * 1024)
            .expect("large-output fixture should exit");

        assert!(output.status.success());
        assert_eq!(output.stdout.len(), 64 * 1024);
        assert_eq!(output.stderr.len(), 32 * 1024);
        assert!(output.stdout_truncated);
        assert!(output.stderr_truncated);
    }

    #[test]
    fn structured_text_rejects_truncated_stdout() {
        let output = run_fixture("large-output", None, 64 * 1024, 32 * 1024)
            .expect("large-output fixture should exit");

        let error = normalize_structured_text_output(output)
            .expect_err("truncated structured output must be rejected");

        assert!(matches!(error, AiExecutionError::OutputLimit { .. }));
    }

    #[test]
    fn cancellation_terminates_the_running_process() {
        let cancellation = AiExecutionCancellation::default();
        let cancellation_for_thread = cancellation.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(100));
            cancellation_for_thread.cancel();
        });
        let started = Instant::now();

        let error = run_fixture("timeout", Some(cancellation), 1024, 1024)
            .expect_err("cancelled fixture should be terminated");

        assert!(matches!(error, AiExecutionError::Cancelled { .. }));
        assert!(started.elapsed() < Duration::from_secs(3));
    }

    #[test]
    #[cfg(unix)]
    fn cancellation_terminates_descendants_that_hold_output_pipes() {
        let cancellation = AiExecutionCancellation::default();
        let cancellation_for_thread = cancellation.clone();
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(100));
            cancellation_for_thread.cancel();
        });
        let started = Instant::now();

        let error = run_fixture("child-tree", Some(cancellation), 1024, 1024)
            .expect_err("cancelled process tree should be terminated");

        assert!(matches!(error, AiExecutionError::Cancelled { .. }));
        assert!(started.elapsed() < Duration::from_secs(3));
    }

    #[test]
    fn builds_runtime_specific_structured_text_arguments_without_a_shell() {
        assert_eq!(
            structured_text_args(AiCliRuntime::Opencode, Some("model/a"), "prompt"),
            ["run", "--model", "model/a", "prompt"]
        );
        assert_eq!(
            structured_text_args(AiCliRuntime::Gemini, Some("gemini-2.5"), "prompt"),
            ["--model", "gemini-2.5", "--prompt", "prompt"]
        );
    }

    fn run_fixture(
        mode: &str,
        cancellation: Option<AiExecutionCancellation>,
        stdout_cap: usize,
        stderr_cap: usize,
    ) -> Result<AiCommandOutput, AiExecutionError> {
        let program = env::current_exe().expect("resolve test binary");
        let args = vec![
            "--exact".to_string(),
            "backend::ai_execution::tests::process_fixture".to_string(),
            "--nocapture".to_string(),
        ];
        let mut options = AiCommandOptions::new(Duration::from_secs(5), stdout_cap, stderr_cap);
        options.environment.push((
            OsString::from("ASSETIWEAVE_AI_EXECUTION_FIXTURE"),
            OsString::from(mode),
        ));
        options.cancellation = cancellation;
        run_cli_command_at_path(&program, &args, options)
    }
}
