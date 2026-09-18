use std::{
    env,
    ffi::{OsStr, OsString},
    fs,
    path::{Path, PathBuf},
    process::{ExitStatus, Stdio},
    time::{Duration, Instant},
};

#[cfg(windows)]
use process_wrap::tokio::JobObject;
#[cfg(unix)]
use process_wrap::tokio::ProcessGroup;
use process_wrap::tokio::{CommandWrap, KillOnDrop};
use tokio::io::AsyncReadExt;
use tokio_util::sync::CancellationToken;

#[derive(Debug)]
pub(crate) struct HostProcessOutput {
    pub(crate) status: ExitStatus,
    pub(crate) stdout: Vec<u8>,
    pub(crate) stderr: Vec<u8>,
    pub(crate) stdout_truncated: bool,
    pub(crate) stderr_truncated: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct HostCommandSpec {
    pub(crate) program: PathBuf,
    pub(crate) args: Vec<String>,
    pub(crate) env: Vec<(String, String)>,
    pub(crate) working_dir: Option<PathBuf>,
    pub(crate) stdin: HostInput,
    pub(crate) timeout: Duration,
    pub(crate) stdout_limit: usize,
    pub(crate) stderr_limit: usize,
}

#[derive(Debug, Clone, Default)]
pub(crate) enum HostInput {
    #[default]
    Null,
    Bytes(Vec<u8>),
}

#[derive(Debug)]
pub(crate) struct HostCommandOutput {
    pub(crate) status: ExitStatus,
    pub(crate) stdout: Vec<u8>,
    pub(crate) stderr: Vec<u8>,
    pub(crate) stdout_truncated: bool,
    pub(crate) stderr_truncated: bool,
    pub(crate) elapsed: Duration,
}

impl HostCommandOutput {
    pub(crate) fn output_limit_error(&self) -> Option<HostProcessError> {
        (self.stdout_truncated || self.stderr_truncated).then_some(
            HostProcessError::OutputLimitExceeded {
                stdout: self.stdout_truncated,
                stderr: self.stderr_truncated,
            },
        )
    }
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum HostProcessError {
    #[error("program not found: {program}")]
    MissingProgram { program: PathBuf },
    #[error("failed to spawn process: {0}")]
    Spawn(String),
    #[error("process output error: {0}")]
    Output(String),
    #[error("process timed out")]
    Timeout {
        stdout: Vec<u8>,
        stderr: Vec<u8>,
        stdout_truncated: bool,
        stderr_truncated: bool,
    },
    #[error("process execution was cancelled")]
    Cancelled,
    #[error("process output limit exceeded (stdout={stdout}, stderr={stderr})")]
    OutputLimitExceeded { stdout: bool, stderr: bool },
    #[cfg_attr(not(test), allow(dead_code))]
    #[error("process cleanup failed: {0}")]
    Cleanup(String),
}

impl From<HostProcessError> for crate::backend::runtime::AppError {
    fn from(error: HostProcessError) -> Self {
        match error {
            HostProcessError::MissingProgram { program } => {
                crate::backend::runtime::AppError::NotFound(format!(
                    "executable not found: {}",
                    program.display()
                ))
            }
            HostProcessError::Spawn(reason) => crate::backend::runtime::AppError::Process(reason),
            HostProcessError::Output(reason) => crate::backend::runtime::AppError::Process(reason),
            HostProcessError::Timeout { .. } => crate::backend::runtime::AppError::Timeout(
                "process execution timed out".to_string(),
            ),
            HostProcessError::Cancelled => crate::backend::runtime::AppError::Cancelled(
                "process execution was cancelled".to_string(),
            ),
            HostProcessError::OutputLimitExceeded { stdout, stderr } => {
                crate::backend::runtime::AppError::Process(format!(
                    "process output limit exceeded (stdout={stdout}, stderr={stderr})"
                ))
            }
            HostProcessError::Cleanup(reason) => crate::backend::runtime::AppError::Process(reason),
        }
    }
}

pub(crate) fn wrap_tokio_command(cmd: tokio::process::Command) -> CommandWrap {
    let mut wrap = CommandWrap::from(cmd);
    #[cfg(unix)]
    {
        wrap.wrap(ProcessGroup::leader());
    }
    #[cfg(windows)]
    {
        wrap.wrap(JobObject);
    }
    wrap.wrap(KillOnDrop);
    wrap
}

pub(crate) fn resolve_host_executable(command_name: &str) -> Option<PathBuf> {
    let command_path = Path::new(command_name);
    if command_path.components().count() > 1 {
        return is_executable_file(command_path).then(|| command_path.to_path_buf());
    }

    if let Ok(path) = which::which(command_name) {
        return Some(path);
    }

    let path_env = env::var_os("PATH");
    let home_dir = dirs::home_dir();
    let search_candidates = host_executable_search_candidates(command_name, home_dir.as_deref());
    resolve_host_executable_from_sources(
        command_name,
        path_env.as_deref(),
        None,
        &search_candidates,
    )
}

pub(crate) fn resolve_host_executable_from_sources(
    command_name: &str,
    path_env: Option<&OsStr>,
    login_shell_candidate: Option<PathBuf>,
    search_candidates: &[PathBuf],
) -> Option<PathBuf> {
    let cwd = env::current_dir().unwrap_or_default();
    if let Ok(path) = which::which_in(command_name, path_env, cwd) {
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

pub(crate) fn host_executable_name(command_name: &str) -> OsString {
    #[cfg(not(windows))]
    {
        OsString::from(command_name)
    }

    #[cfg(windows)]
    {
        OsString::from(format!("{command_name}.exe"))
    }
}

pub(crate) fn host_executable_search_candidates(
    command_name: &str,
    home_dir: Option<&Path>,
) -> Vec<PathBuf> {
    let executable = host_executable_name(command_name);
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

pub(crate) type HostStdoutLineListener = std::sync::Arc<dyn Fn(&str) + Send + Sync + 'static>;

const OUTPUT_DRAIN_AFTER_TERMINATION: Duration = Duration::from_millis(300);

pub(crate) async fn run_host_command(
    spec: HostCommandSpec,
    cancellation: CancellationToken,
) -> Result<HostCommandOutput, HostProcessError> {
    run_host_command_async_streaming(spec, Some(&cancellation), None).await
}

pub(crate) async fn run_host_command_async(
    spec: HostCommandSpec,
    cancellation: Option<&CancellationToken>,
) -> Result<HostCommandOutput, HostProcessError> {
    run_host_command_async_streaming(spec, cancellation, None).await
}

pub(crate) async fn run_host_command_async_streaming(
    spec: HostCommandSpec,
    cancellation: Option<&CancellationToken>,
    stdout_listener: Option<HostStdoutLineListener>,
) -> Result<HostCommandOutput, HostProcessError> {
    if let Some(token) = cancellation {
        if token.is_cancelled() {
            return Err(HostProcessError::Cancelled);
        }
    }

    let mut cmd = build_tokio_host_command(&spec)?;
    let has_stdin_bytes = matches!(&spec.stdin, HostInput::Bytes(_));
    cmd.stdin(if has_stdin_bytes {
        Stdio::piped()
    } else {
        Stdio::null()
    });
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    let mut wrap = wrap_tokio_command(cmd);
    let mut child = wrap
        .spawn()
        .map_err(|e| HostProcessError::Spawn(e.to_string()))?;

    if let HostInput::Bytes(bytes) = spec.stdin {
        if let Some(mut stdin) = child.stdin().take() {
            tokio::spawn(async move {
                let _ = tokio::io::AsyncWriteExt::write_all(&mut stdin, &bytes).await;
            });
        }
    }

    let stdout_pipe = child.stdout().take();
    let stderr_pipe = child.stderr().take();

    let stdout_limit = spec.stdout_limit;
    let stderr_limit = spec.stderr_limit;

    let mut stdout_task = tokio::spawn(async move {
        match stdout_pipe {
            Some(r) => {
                read_stream_capped_and_drain_streaming(r, stdout_limit, stdout_listener).await
            }
            None => (Vec::new(), false),
        }
    });
    let mut stderr_task = tokio::spawn(async move {
        match stderr_pipe {
            Some(r) => read_stream_capped_and_drain(r, stderr_limit).await,
            None => (Vec::new(), false),
        }
    });

    let started = Instant::now();
    let timeout_sleep = tokio::time::sleep(spec.timeout);
    tokio::pin!(timeout_sleep);

    enum ExitReason {
        Exited(Result<ExitStatus, std::io::Error>),
        TimedOut,
        Cancelled,
    }

    let reason = tokio::select! {
        res = child.wait() => ExitReason::Exited(res),
        _ = &mut timeout_sleep => ExitReason::TimedOut,
        _ = async {
            if let Some(token) = cancellation {
                token.cancelled().await;
            } else {
                std::future::pending::<()>().await;
            }
        } => ExitReason::Cancelled,
    };

    match reason {
        ExitReason::Exited(res) => {
            let status = res.map_err(|e| HostProcessError::Output(e.to_string()))?;
            let (stdout_res, stderr_res) = tokio::select! {
                joined = async {
                    let out = (&mut stdout_task).await.unwrap_or_default();
                    let err = (&mut stderr_task).await.unwrap_or_default();
                    (out, err)
                } => joined,
                _ = tokio::time::sleep(OUTPUT_DRAIN_AFTER_TERMINATION) => {
                    let _ = child.start_kill();
                    let out = stdout_task.await.unwrap_or_default();
                    let err = stderr_task.await.unwrap_or_default();
                    (out, err)
                }
            };
            Ok(HostCommandOutput {
                status,
                stdout: stdout_res.0,
                stderr: stderr_res.0,
                stdout_truncated: stdout_res.1,
                stderr_truncated: stderr_res.1,
                elapsed: started.elapsed(),
            })
        }
        ExitReason::TimedOut => {
            let _ = child.start_kill();
            let _ = child.wait().await;
            let out = stdout_task.await.unwrap_or_default();
            let err = stderr_task.await.unwrap_or_default();
            Err(HostProcessError::Timeout {
                stdout: out.0,
                stderr: err.0,
                stdout_truncated: out.1,
                stderr_truncated: err.1,
            })
        }
        ExitReason::Cancelled => {
            let _ = child.start_kill();
            let _ = child.wait().await;
            let drained = tokio::time::timeout(OUTPUT_DRAIN_AFTER_TERMINATION, async {
                let _ = (&mut stdout_task).await;
                let _ = (&mut stderr_task).await;
            })
            .await
            .is_ok();
            if !drained {
                stdout_task.abort();
                stderr_task.abort();
                let _ = stdout_task.await;
                let _ = stderr_task.await;
            }
            Err(HostProcessError::Cancelled)
        }
    }
}

fn build_tokio_host_command(
    spec: &HostCommandSpec,
) -> Result<tokio::process::Command, HostProcessError> {
    let resolved = if spec.program.components().count() > 1 {
        if !is_executable_file(&spec.program) {
            return Err(HostProcessError::MissingProgram {
                program: spec.program.clone(),
            });
        }
        spec.program.clone()
    } else {
        resolve_host_executable(&spec.program.to_string_lossy()).ok_or_else(|| {
            HostProcessError::MissingProgram {
                program: spec.program.clone(),
            }
        })?
    };
    let mut command = tokio::process::Command::new(resolved);
    command
        .args(&spec.args)
        .envs(spec.env.iter().map(|(key, value)| (key, value)));
    if let Some(working_dir) = spec.working_dir.as_deref() {
        command.current_dir(working_dir);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000);
    }
    Ok(command)
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

async fn read_stream_capped_and_drain<R: tokio::io::AsyncRead + Unpin>(
    reader: R,
    cap: usize,
) -> (Vec<u8>, bool) {
    read_stream_capped_and_drain_streaming(reader, cap, None).await
}

async fn read_stream_capped_and_drain_streaming<R: tokio::io::AsyncRead + Unpin>(
    mut reader: R,
    cap: usize,
    listener: Option<HostStdoutLineListener>,
) -> (Vec<u8>, bool) {
    let mut output = Vec::with_capacity(cap.min(8192));
    let mut buffer = [0_u8; 8192];
    let mut line_buffer = Vec::new();
    let mut truncated = false;
    loop {
        let read = match reader.read(&mut buffer).await {
            Ok(0) => break,
            Ok(n) => n,
            Err(_) => break,
        };
        let remaining = cap.saturating_sub(output.len());
        let retained = remaining.min(read);
        output.extend_from_slice(&buffer[..retained]);
        truncated |= retained < read;

        if let Some(ref cb) = listener {
            let mut start = 0;
            for (i, &b) in buffer[..read].iter().enumerate() {
                if b == b'\n' {
                    line_buffer.extend_from_slice(&buffer[start..i]);
                    let line_str = String::from_utf8_lossy(&line_buffer);
                    cb(line_str.trim_end_matches('\r'));
                    line_buffer.clear();
                    start = i + 1;
                }
            }
            if start < read {
                if line_buffer.len() + (read - start) <= 64 * 1024 {
                    line_buffer.extend_from_slice(&buffer[start..read]);
                }
            }
        }
    }
    if let Some(ref cb) = listener {
        if !line_buffer.is_empty() {
            let line_str = String::from_utf8_lossy(&line_buffer);
            cb(line_str.trim_end_matches('\r'));
        }
    }
    (output, truncated)
}

#[cfg(test)]
#[path = "host_process_tests.rs"]
mod tests;
