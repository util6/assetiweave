use crate::backend::host_process::{
    run_command_with_control, run_command_with_timeout, HostProcessControl, HostProcessError,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{
    env,
    ffi::{OsStr, OsString},
    fmt, fs,
    path::{Path, PathBuf},
    process::{Command, ExitStatus, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};

const MAX_PROMPT_BYTES: usize = 1_000_000;
const LOGIN_SHELL_TIMEOUT: Duration = Duration::from_secs(5);
const DISCOVERY_OUTPUT_CAP: usize = 8 * 1024;

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

#[derive(Clone, Debug, Default)]
pub(crate) struct AiExecutionCancellation {
    cancelled: Arc<AtomicBool>,
}

impl AiExecutionCancellation {
    pub(crate) fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
    }

    pub(crate) fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Acquire)
    }

    fn flag(&self) -> &AtomicBool {
        self.cancelled.as_ref()
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

#[derive(Debug)]
pub(crate) enum AiExecutionError {
    RuntimeUnavailable {
        command_name: String,
    },
    Spawn {
        program: PathBuf,
        message: String,
    },
    Output {
        program: PathBuf,
        message: String,
    },
    Timeout {
        program: PathBuf,
        timeout: Duration,
        stdout: Vec<u8>,
        stderr: Vec<u8>,
        stdout_truncated: bool,
        stderr_truncated: bool,
    },
    Cancelled {
        program: PathBuf,
        stdout: Vec<u8>,
        stderr: Vec<u8>,
        stdout_truncated: bool,
        stderr_truncated: bool,
    },
    OutputLimit(AiCommandOutput),
    CommandFailed(AiCommandOutput),
    EmptyOutput {
        program: PathBuf,
    },
    InvalidPrompt(String),
    InvalidModel(String),
}

impl fmt::Display for AiExecutionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::RuntimeUnavailable { command_name } => write!(
                formatter,
                "{command_name} was not found on this host. Install it and make `{command_name}` available on PATH or from a login shell."
            ),
            Self::Spawn { program, message } => {
                write!(formatter, "failed to start {}: {message}", program.display())
            }
            Self::Output { message, .. } => formatter.write_str(message),
            Self::Timeout {
                program, timeout, ..
            } => write!(
                formatter,
                "{} timed out after {} seconds",
                program.display(),
                timeout.as_secs()
            ),
            Self::Cancelled { program, .. } => {
                write!(formatter, "{} was cancelled", program.display())
            }
            Self::OutputLimit(output) => write!(
                formatter,
                "{} exceeded the configured output limit",
                output.program.display()
            ),
            Self::CommandFailed(output) => write!(
                formatter,
                "{} failed with status {}",
                output.program.display(),
                output.status
            ),
            Self::EmptyOutput { program } => {
                write!(formatter, "{} returned empty output", program.display())
            }
            Self::InvalidPrompt(message) | Self::InvalidModel(message) => {
                formatter.write_str(message)
            }
        }
    }
}

impl std::error::Error for AiExecutionError {}

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
        return Err(AiExecutionError::OutputLimit(output));
    }

    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if text.is_empty() {
        return Err(AiExecutionError::EmptyOutput {
            program: output.program,
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
    let path_env = env::var_os("PATH");
    let login_shell_candidate = find_command_with_login_shell(command_name);
    let home_dir = dirs::home_dir();
    let search_candidates = cli_search_candidates(command_name, home_dir.as_deref());
    resolve_cli_executable_from_sources(
        command_name,
        path_env.as_deref(),
        login_shell_candidate,
        &search_candidates,
    )
    .ok_or_else(|| AiExecutionError::RuntimeUnavailable {
        command_name: command_name.to_string(),
    })
}

pub(crate) fn resolve_cli_executable_from_sources(
    command_name: &str,
    path_env: Option<&OsStr>,
    login_shell_candidate: Option<PathBuf>,
    search_candidates: &[PathBuf],
) -> Option<PathBuf> {
    if let Some(path) = find_program_on_path(command_name, path_env) {
        return Some(path);
    }

    if let Some(path) = login_shell_candidate.filter(|path| is_executable_file(path)) {
        return Some(path);
    }

    search_candidates
        .iter()
        .find(|candidate| is_executable_file(candidate))
        .cloned()
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

pub(crate) fn executable_name(command_name: &str) -> OsString {
    #[cfg(not(windows))]
    {
        OsString::from(command_name)
    }

    #[cfg(windows)]
    {
        OsString::from(format!("{command_name}.exe"))
    }
}

pub(crate) fn cli_search_candidates(command_name: &str, home_dir: Option<&Path>) -> Vec<PathBuf> {
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
    let mut command = Command::new(shell);
    command
        .args(["-lc", &script])
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let output = run_command_with_timeout(
        &mut command,
        LOGIN_SHELL_TIMEOUT,
        DISCOVERY_OUTPUT_CAP,
        DISCOVERY_OUTPUT_CAP,
    )
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

fn first_nonempty_line(bytes: &[u8]) -> Option<String> {
    String::from_utf8_lossy(bytes)
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(str::to_string)
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

        assert!(matches!(error, AiExecutionError::OutputLimit(_)));
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
