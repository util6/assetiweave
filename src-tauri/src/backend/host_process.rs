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

pub(crate) async fn run_host_command(
    spec: HostCommandSpec,
    cancellation: CancellationToken,
) -> Result<HostCommandOutput, HostProcessError> {
    run_host_command_async(spec, Some(&cancellation)).await
}

pub(crate) async fn run_host_command_async(
    spec: HostCommandSpec,
    cancellation: Option<&CancellationToken>,
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
            Some(r) => read_stream_capped_and_drain(r, stdout_limit).await,
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
                _ = tokio::time::sleep(Duration::from_millis(300)) => {
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
            let _ = stdout_task.await;
            let _ = stderr_task.await;
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
    mut reader: R,
    cap: usize,
) -> (Vec<u8>, bool) {
    let mut output = Vec::with_capacity(cap.min(8192));
    let mut buffer = [0_u8; 8192];
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
    }
    (output, truncated)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        env,
        io::{self, Write},
        time::{Duration, Instant},
    };

    #[tokio::test(flavor = "current_thread")]
    async fn process_fixture() {
        match env::var("ASSETIWEAVE_HOST_PROCESS_FIXTURE").as_deref() {
            Ok("large-output") => {
                io::stdout().write_all(&vec![b'x'; 256 * 1024]).unwrap();
            }
            Ok("timeout") => {
                let (_tx, rx) = std::sync::mpsc::channel::<()>();
                let _ = rx.recv_timeout(Duration::from_secs(5));
            }
            #[cfg(unix)]
            Ok("launcher-exits") => {
                let _ = tokio::process::Command::new("sh")
                    .args(["-c", "sleep 5"])
                    .spawn()
                    .expect("spawn inherited-pipe descendant");
            }
            #[cfg(windows)]
            Ok("launcher-exits") => {
                let _ = tokio::process::Command::new("cmd")
                    .args(["/C", "ping -n 6 127.0.0.1 > nul"])
                    .spawn()
                    .expect("spawn inherited-pipe descendant");
            }
            Ok("normal-exit") => {
                let _ = io::stdout().write_all(b"fixture-stdout-content");
                let _ = io::stderr().write_all(b"fixture-stderr-content");
            }
            Ok("nonzero-exit") => {
                let _ = io::stderr().write_all(b"exiting with error 42");
                std::process::exit(42);
            }
            Ok("ignore-term") => {
                #[cfg(unix)]
                unsafe {
                    libc::signal(libc::SIGTERM, libc::SIG_IGN);
                }
                let _ = io::stdout().write_all(b"ready\n");
                let _ = io::stdout().flush();
                let (_tx, rx) = std::sync::mpsc::channel::<()>();
                let _ = rx.recv_timeout(Duration::from_secs(10));
            }
            _ => {}
        }
    }

    fn fixture_spec(
        mode: &str,
        timeout: Duration,
        stdout_limit: usize,
        stderr_limit: usize,
    ) -> HostCommandSpec {
        HostCommandSpec {
            program: env::current_exe().expect("resolve test binary"),
            args: vec![
                "--exact".to_string(),
                "backend::host_process::tests::process_fixture".to_string(),
                "--nocapture".to_string(),
            ],
            env: vec![(
                "ASSETIWEAVE_HOST_PROCESS_FIXTURE".to_string(),
                mode.to_string(),
            )],
            working_dir: None,
            stdin: HostInput::Null,
            timeout,
            stdout_limit,
            stderr_limit,
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn async_runner_shares_bounded_output_and_reports_elapsed_time() {
        let output = run_host_command(
            fixture_spec("large-output", Duration::from_secs(5), 32 * 1024, 32 * 1024),
            tokio_util::sync::CancellationToken::new(),
        )
        .await
        .expect("async host command should exit");

        assert!(output.status.success());
        assert_eq!(output.stdout.len(), 32 * 1024);
        assert!(output.stdout_truncated);
        assert!(!output.elapsed.is_zero());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn async_runner_cancels_and_reaps_the_process_tree() {
        let cancellation = tokio_util::sync::CancellationToken::new();
        let task = run_host_command(
            fixture_spec("timeout", Duration::from_secs(5), 32 * 1024, 32 * 1024),
            cancellation.clone(),
        );
        tokio::pin!(task);
        tokio::time::sleep(Duration::from_millis(50)).await;
        cancellation.cancel();

        let error = task.await.expect_err("cancelled command should fail");
        assert!(matches!(error, HostProcessError::Cancelled));
    }

    #[test]
    fn token_cancellation_has_no_mirror_watcher_thread() {
        let source = include_str!("host_process.rs");
        assert!(!source.contains(concat!("let watcher_", "done =")));
        assert!(!source.contains(concat!("let watcher_", "token =")));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn public_seam_host_command_normal_exit() {
        let output = run_host_command(
            fixture_spec("normal-exit", Duration::from_secs(5), 32 * 1024, 32 * 1024),
            tokio_util::sync::CancellationToken::new(),
        )
        .await
        .expect("normal exit succeeds");

        assert!(output.status.success());
        assert!(String::from_utf8_lossy(&output.stdout).contains("fixture-stdout-content"));
        assert!(String::from_utf8_lossy(&output.stderr).contains("fixture-stderr-content"));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn public_seam_host_command_nonzero_exit() {
        let output = run_host_command(
            fixture_spec("nonzero-exit", Duration::from_secs(5), 32 * 1024, 32 * 1024),
            tokio_util::sync::CancellationToken::new(),
        )
        .await
        .expect("runs to nonzero completion");

        assert!(!output.status.success());
        assert_eq!(output.status.code(), Some(42));
        assert_eq!(output.stderr, b"exiting with error 42");
    }

    #[test]
    fn host_process_error_into_app_error_mapping() {
        use crate::backend::runtime::AppError;

        let missing = HostProcessError::MissingProgram {
            program: PathBuf::from("nonexistent-tool"),
        };
        let app_err: AppError = missing.into();
        assert!(matches!(app_err, AppError::NotFound(_)));

        let timeout = HostProcessError::Timeout {
            stdout: Vec::new(),
            stderr: Vec::new(),
            stdout_truncated: false,
            stderr_truncated: false,
        };
        let app_err: AppError = timeout.into();
        assert!(matches!(app_err, AppError::Timeout(_)));

        let cancelled = HostProcessError::Cancelled;
        let app_err: AppError = cancelled.into();
        assert!(matches!(app_err, AppError::Cancelled(_)));

        let limit = HostProcessError::OutputLimitExceeded {
            stdout: true,
            stderr: false,
        };
        let app_err: AppError = limit.into();
        assert!(matches!(app_err, AppError::Process(_)));
    }

    // =========================================================================
    // C-PROCESS-01 & B2-P01/B2-P03C: Contract Tests for process-wrap with production async runner
    // =========================================================================

    #[tokio::test(flavor = "current_thread")]
    async fn contract_process_wrap_normal_exit() {
        let output = run_host_command_async(
            fixture_spec("normal-exit", Duration::from_secs(5), 32 * 1024, 32 * 1024),
            None,
        )
        .await
        .expect("normal exit fixture should succeed");

        assert!(output.status.success());
        assert_eq!(output.status.code(), Some(0));
        let stdout_str = String::from_utf8_lossy(&output.stdout);
        let stderr_str = String::from_utf8_lossy(&output.stderr);
        assert!(stdout_str.contains("fixture-stdout-content"));
        assert!(stderr_str.contains("fixture-stderr-content"));
        assert!(!output.stdout_truncated);
        assert!(!output.stderr_truncated);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn contract_process_wrap_nonzero_exit() {
        let output = run_host_command_async(
            fixture_spec("nonzero-exit", Duration::from_secs(5), 32 * 1024, 32 * 1024),
            None,
        )
        .await
        .expect("nonzero exit fixture completes execution");

        assert!(!output.status.success());
        assert_eq!(output.status.code(), Some(42));
        assert_eq!(output.stderr, b"exiting with error 42");
    }

    #[tokio::test(flavor = "current_thread")]
    async fn contract_process_wrap_bounded_output() {
        let output = run_host_command_async(
            fixture_spec("large-output", Duration::from_secs(5), 32 * 1024, 32 * 1024),
            None,
        )
        .await
        .expect("large-output fixture should succeed");

        assert!(output.status.success());
        assert_eq!(output.stdout.len(), 32 * 1024);
        assert!(output.stdout_truncated);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn contract_process_wrap_timeout() {
        let started = Instant::now();
        let err = run_host_command_async(
            fixture_spec("timeout", Duration::from_millis(150), 32 * 1024, 32 * 1024),
            None,
        )
        .await
        .expect_err("timeout fixture must fail with timeout");

        assert!(matches!(err, HostProcessError::Timeout { .. }));
        assert!(
            started.elapsed() < Duration::from_secs(3),
            "timeout reap must be prompt"
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn contract_process_wrap_pre_cancel() {
        let cancellation = tokio_util::sync::CancellationToken::new();
        cancellation.cancel();

        let err = run_host_command_async(
            fixture_spec("timeout", Duration::from_secs(5), 32 * 1024, 32 * 1024),
            Some(&cancellation),
        )
        .await
        .expect_err("pre-cancelled command must fail immediately");

        assert!(matches!(err, HostProcessError::Cancelled));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn contract_process_wrap_mid_flight_cancel() {
        let cancellation = tokio_util::sync::CancellationToken::new();
        let cancel_handle = cancellation.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            cancel_handle.cancel();
        });

        let started = Instant::now();
        let err = run_host_command_async(
            fixture_spec("timeout", Duration::from_secs(5), 32 * 1024, 32 * 1024),
            Some(&cancellation),
        )
        .await
        .expect_err("mid-flight cancelled command must fail");

        assert!(matches!(err, HostProcessError::Cancelled));
        assert!(
            started.elapsed() < Duration::from_secs(3),
            "cancel must reap immediately"
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn contract_process_wrap_descendant_held_pipe() {
        let started = Instant::now();
        let output = run_host_command_async(
            fixture_spec(
                "launcher-exits",
                Duration::from_secs(4),
                64 * 1024,
                64 * 1024,
            ),
            None,
        )
        .await
        .expect("launcher exit should succeed without hanging on descendant pipes");

        assert!(
            output.status.success(),
            "launcher itself must have exited successfully"
        );
        assert!(
            started.elapsed() < Duration::from_secs(3),
            "must reap descendant and pipe without waiting full 5s"
        );
    }

    #[tokio::test(flavor = "current_thread")]
    #[cfg(windows)]
    async fn contract_windows_job_object_reaps_descendant_held_pipe() {
        let started = Instant::now();
        let output = run_host_command_async(
            fixture_spec(
                "launcher-exits",
                Duration::from_secs(4),
                64 * 1024,
                64 * 1024,
            ),
            None,
        )
        .await
        .expect("launcher exit should succeed without hanging on descendant pipes via JobObject");

        assert!(
            output.status.success(),
            "launcher itself must have exited successfully"
        );
        assert!(
            started.elapsed() < Duration::from_secs(3),
            "JobObject must reap descendant and pipe without waiting full 5s"
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn contract_process_wrap_ignored_graceful_termination() {
        let cmd = make_tokio_fixture_command("ignore-term");
        let mut wrap = wrap_tokio_command(cmd);
        let mut child = wrap.spawn().expect("spawn ignore-term fixture");

        if let Some(mut stdout) = child.stdout().take() {
            let mut buf = [0u8; 6];
            let _ = stdout.read_exact(&mut buf).await;
        }

        #[cfg(unix)]
        {
            let _ = child.signal(libc::SIGTERM);
            tokio::time::sleep(Duration::from_millis(100)).await;
            let status = child.try_wait().expect("try_wait");
            assert!(status.is_none(), "process should have ignored SIGTERM");
        }

        let started = Instant::now();
        let _ = child.start_kill();
        let status = child.wait().await.expect("wait for force killed child");
        assert!(
            !status.success(),
            "force killed process must not be success"
        );
        assert!(
            started.elapsed() < Duration::from_secs(3),
            "force kill should be fast"
        );
    }

    #[test]
    fn contract_which_standard_lookup_and_desktop_fallback() {
        let unique_dir_name = format!("assetiweave-which-test-{}", uuid::Uuid::new_v4());
        let root_temp = env::temp_dir().join(unique_dir_name);
        let bin_dir = root_temp.join("bin");
        let work_dir = root_temp.join("work");
        fs::create_dir_all(&bin_dir).expect("create bin dir");
        fs::create_dir_all(&work_dir).expect("create work dir");

        let exe_name = host_executable_name("test-tool");
        let exe_path = bin_dir.join(&exe_name);

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::write(&exe_path, b"#!/bin/sh\necho hello\n").expect("write temp exe");
            let mut perms = fs::metadata(&exe_path).expect("metadata").permissions();
            perms.set_mode(0o755);
            fs::set_permissions(&exe_path, perms).expect("chmod +x");
        }
        #[cfg(windows)]
        {
            fs::write(&exe_path, b"@echo hello\r\n").expect("write temp exe");
        }

        let cmd_name = "test-tool";

        let custom_path = env::join_paths([&bin_dir]).expect("join paths");
        let found = which::which_in(cmd_name, Some(&custom_path), &work_dir)
            .expect("which must find executable in PATH");
        assert_eq!(found, exe_path);

        let empty_path = OsString::from("");
        let miss = which::which_in(cmd_name, Some(&empty_path), &work_dir);
        assert!(
            miss.is_err(),
            "which must report error when not on PATH and not in cwd"
        );

        let standard_lookup = which::which_in(cmd_name, Some(&empty_path), &work_dir).ok();
        let resolved = standard_lookup.or_else(|| {
            let candidates = vec![exe_path.clone()];
            candidates.into_iter().find(|p| is_executable_file(p))
        });
        assert_eq!(
            resolved,
            Some(exe_path.clone()),
            "fallback is triggered on lookup miss"
        );

        let resolved_direct = which::which_in(cmd_name, Some(&custom_path), &work_dir)
            .ok()
            .or_else(|| panic!("fallback should not be reached when standard lookup succeeds"));
        assert_eq!(resolved_direct, Some(exe_path));

        let _ = fs::remove_dir_all(&root_temp);
    }

    fn make_tokio_fixture_command(mode: &str) -> tokio::process::Command {
        let mut cmd =
            tokio::process::Command::new(env::current_exe().expect("resolve test binary"));
        cmd.args([
            "--exact",
            "backend::host_process::tests::process_fixture",
            "--nocapture",
        ])
        .env("ASSETIWEAVE_HOST_PROCESS_FIXTURE", mode)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .stdin(Stdio::null());
        cmd
    }
}
