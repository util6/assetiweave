use std::{
    env,
    ffi::{OsStr, OsString},
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, ExitStatus, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};

const LOGIN_SHELL_TIMEOUT: Duration = Duration::from_secs(5);
const DISCOVERY_OUTPUT_CAP: usize = 8 * 1024;

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

#[derive(Debug)]
pub(crate) enum HostProcessError {
    MissingProgram {
        program: PathBuf,
    },
    Spawn(String),
    Output(String),
    Timeout {
        stdout: Vec<u8>,
        stderr: Vec<u8>,
        stdout_truncated: bool,
        stderr_truncated: bool,
    },
    Cancelled,
    OutputLimitExceeded {
        stdout: bool,
        stderr: bool,
    },
    Cleanup(String),
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct HostProcessControl<'a> {
    pub(crate) timeout: Duration,
    pub(crate) stdout_cap: usize,
    pub(crate) stderr_cap: usize,
    pub(crate) cancellation: Option<&'a AtomicBool>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum HostProcessSignal {
    Terminate,
    Kill,
}

pub(crate) fn resolve_host_executable(command_name: &str) -> Option<PathBuf> {
    let command_path = Path::new(command_name);
    if command_path.components().count() > 1 {
        return is_executable_file(command_path).then(|| command_path.to_path_buf());
    }

    let path_env = env::var_os("PATH");
    let login_shell_candidate = find_command_with_login_shell(command_name);
    let home_dir = dirs::home_dir();
    let search_candidates = host_executable_search_candidates(command_name, home_dir.as_deref());
    resolve_host_executable_from_sources(
        command_name,
        path_env.as_deref(),
        login_shell_candidate,
        &search_candidates,
    )
}

pub(crate) fn resolve_host_executable_from_sources(
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

pub(crate) fn run_command_with_timeout(
    command: &mut Command,
    timeout: Duration,
    stdout_cap: usize,
    stderr_cap: usize,
) -> Result<HostProcessOutput, HostProcessError> {
    run_command_with_control(
        command,
        HostProcessControl {
            timeout,
            stdout_cap,
            stderr_cap,
            cancellation: None,
        },
    )
}

/// Build and execute a host command inside the process boundary. Application
/// code should use this helper instead of constructing `std::process::Command`
/// so executable lookup, output limits and timeout behavior remain uniform.
pub(crate) fn run_program_with_timeout(
    program: &Path,
    args: &[String],
    current_dir: Option<&Path>,
    timeout: Duration,
    stdout_cap: usize,
    stderr_cap: usize,
) -> Result<HostProcessOutput, HostProcessError> {
    run_program_with_cancellation(
        program,
        args,
        current_dir,
        timeout,
        stdout_cap,
        stderr_cap,
        None,
    )
}

/// Execute a bounded host command while observing a task cancellation token.
/// The watcher only flips the existing process-control flag; the command
/// runner remains responsible for terminating and reaping the process group.
pub(crate) fn run_program_with_cancellation(
    program: &Path,
    args: &[String],
    current_dir: Option<&Path>,
    timeout: Duration,
    stdout_cap: usize,
    stderr_cap: usize,
    cancellation: Option<&tokio_util::sync::CancellationToken>,
) -> Result<HostProcessOutput, HostProcessError> {
    let spec = HostCommandSpec {
        program: program.to_path_buf(),
        args: args.to_vec(),
        env: Vec::new(),
        working_dir: current_dir.map(Path::to_path_buf),
        // An explicit empty input stream gives one-shot tools a deterministic
        // EOF while still exercising the same bounded stdin path as callers
        // that provide request bytes.
        stdin: HostInput::Bytes(Vec::new()),
        timeout,
        stdout_limit: stdout_cap,
        stderr_limit: stderr_cap,
    };

    let Some(cancellation) = cancellation else {
        return run_host_command_blocking(spec).map(|output| HostProcessOutput {
            status: output.status,
            stdout: output.stdout,
            stderr: output.stderr,
            stdout_truncated: output.stdout_truncated,
            stderr_truncated: output.stderr_truncated,
        });
    };

    let cancellation_flag = Arc::new(AtomicBool::new(cancellation.is_cancelled()));
    let watcher_done = Arc::new(AtomicBool::new(false));
    let watcher_flag = cancellation_flag.clone();
    let watcher_done_flag = watcher_done.clone();
    let watcher_token = cancellation.clone();
    let watcher = thread::spawn(move || {
        while !watcher_done_flag.load(Ordering::Acquire) {
            if watcher_token.is_cancelled() {
                watcher_flag.store(true, Ordering::Release);
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }
    });
    let result =
        run_host_command_blocking_with_cancellation(spec, Some(&cancellation_flag)).map(|output| {
            HostProcessOutput {
                status: output.status,
                stdout: output.stdout,
                stderr: output.stderr,
                stdout_truncated: output.stdout_truncated,
                stderr_truncated: output.stderr_truncated,
            }
        });
    watcher_done.store(true, Ordering::Release);
    let _ = watcher.join();
    result
}

pub(crate) fn run_host_command_blocking(
    spec: HostCommandSpec,
) -> Result<HostCommandOutput, HostProcessError> {
    run_host_command_blocking_with_cancellation(spec, None)
}

fn run_host_command_blocking_with_cancellation(
    spec: HostCommandSpec,
    cancellation: Option<&AtomicBool>,
) -> Result<HostCommandOutput, HostProcessError> {
    let mut command = build_host_command(&spec)?;
    let started = Instant::now();
    let output = run_command_with_control_and_input(
        &mut command,
        HostProcessControl {
            timeout: spec.timeout,
            stdout_cap: spec.stdout_limit,
            stderr_cap: spec.stderr_limit,
            cancellation,
        },
        spec.stdin,
    )?;
    Ok(HostCommandOutput {
        status: output.status,
        stdout: output.stdout,
        stderr: output.stderr,
        stdout_truncated: output.stdout_truncated,
        stderr_truncated: output.stderr_truncated,
        elapsed: started.elapsed(),
    })
}

pub(crate) async fn run_host_command(
    spec: HostCommandSpec,
    cancellation: tokio_util::sync::CancellationToken,
) -> Result<HostCommandOutput, HostProcessError> {
    let cancellation_flag = Arc::new(AtomicBool::new(cancellation.is_cancelled()));
    let worker_cancellation_flag = cancellation_flag.clone();
    let join = tokio::task::spawn_blocking(move || {
        let mut command = build_host_command(&spec)?;
        let started = Instant::now();
        let output = run_command_with_control_and_input(
            &mut command,
            HostProcessControl {
                timeout: spec.timeout,
                stdout_cap: spec.stdout_limit,
                stderr_cap: spec.stderr_limit,
                cancellation: Some(&worker_cancellation_flag),
            },
            spec.stdin,
        )?;
        Ok(HostCommandOutput {
            status: output.status,
            stdout: output.stdout,
            stderr: output.stderr,
            stdout_truncated: output.stdout_truncated,
            stderr_truncated: output.stderr_truncated,
            elapsed: started.elapsed(),
        })
    });
    tokio::pin!(join);

    tokio::select! {
        output = &mut join => output
            .map_err(|error| HostProcessError::Output(format!("host command worker failed: {error}")))?,
        _ = cancellation.cancelled() => {
            cancellation_flag.store(true, Ordering::Release);
            join.await
                .map_err(|error| HostProcessError::Output(format!("host command worker failed: {error}")))?
        }
    }
}

fn build_host_command(spec: &HostCommandSpec) -> Result<Command, HostProcessError> {
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
    let mut command = Command::new(resolved);
    command
        .args(&spec.args)
        .envs(spec.env.iter().map(|(key, value)| (key, value)));
    if let Some(working_dir) = spec.working_dir.as_deref() {
        command.current_dir(working_dir);
    }
    Ok(command)
}

pub(crate) fn run_command_with_control(
    command: &mut Command,
    control: HostProcessControl<'_>,
) -> Result<HostProcessOutput, HostProcessError> {
    run_command_with_control_and_input(command, control, HostInput::Null)
}

fn run_command_with_control_and_input(
    command: &mut Command,
    control: HostProcessControl<'_>,
    input: HostInput,
) -> Result<HostProcessOutput, HostProcessError> {
    if is_cancelled(control.cancellation) {
        return Err(HostProcessError::Cancelled);
    }

    configure_process_tree(command);
    let stdin = if matches!(&input, HostInput::Bytes(_)) {
        Stdio::piped()
    } else {
        Stdio::null()
    };
    let mut child = command
        .stdin(stdin)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| HostProcessError::Spawn(error.to_string()))?;
    let _stdin_writer = match input {
        HostInput::Null => None,
        HostInput::Bytes(bytes) => child.stdin.take().map(|mut stdin| {
            thread::spawn(move || {
                let _ = stdin.write_all(&bytes);
            })
        }),
    };
    let Some(stdout) = child.stdout.take() else {
        if let Err(error) = cleanup_child_tree(&mut child) {
            return Err(HostProcessError::Cleanup(error));
        }
        return Err(HostProcessError::Output(
            "process stdout was not available".to_string(),
        ));
    };
    let Some(stderr) = child.stderr.take() else {
        if let Err(error) = cleanup_child_tree(&mut child) {
            return Err(HostProcessError::Cleanup(error));
        }
        return Err(HostProcessError::Output(
            "process stderr was not available".to_string(),
        ));
    };
    let stdout_reader = thread::spawn(move || read_capped_and_drain(stdout, control.stdout_cap));
    let stderr_reader = thread::spawn(move || read_capped_and_drain(stderr, control.stderr_cap));
    let started = Instant::now();

    loop {
        let status = match child.try_wait() {
            Ok(status) => status,
            Err(error) => {
                let cleanup = cleanup_child_tree(&mut child);
                let _ = join_output_reader(stdout_reader, "stdout");
                let _ = join_output_reader(stderr_reader, "stderr");
                if let Err(cleanup) = cleanup {
                    return Err(HostProcessError::Cleanup(cleanup));
                }
                return Err(HostProcessError::Output(error.to_string()));
            }
        };
        if let Some(status) = status {
            // A launcher may exit successfully while a descendant keeps the
            // inherited stdout/stderr pipes open. Kill the owned process group
            // before joining readers so a normal exit cannot wait forever on a
            // descendant that escaped the launcher's lifecycle.
            if !stdout_reader.is_finished() || !stderr_reader.is_finished() {
                if let Err(error) = signal_process_tree(child.id(), HostProcessSignal::Kill) {
                    let _ = join_output_reader(stdout_reader, "stdout");
                    let _ = join_output_reader(stderr_reader, "stderr");
                    return Err(HostProcessError::Cleanup(error));
                }
            }
            let (stdout, stdout_truncated) = join_output_reader(stdout_reader, "stdout")?;
            let (stderr, stderr_truncated) = join_output_reader(stderr_reader, "stderr")?;
            return Ok(HostProcessOutput {
                status,
                stdout,
                stderr,
                stdout_truncated,
                stderr_truncated,
            });
        }

        if is_cancelled(control.cancellation) {
            let cleanup = cleanup_child_tree(&mut child);
            let _ = join_output_reader(stdout_reader, "stdout")?;
            let _ = join_output_reader(stderr_reader, "stderr")?;
            if let Err(error) = cleanup {
                return Err(HostProcessError::Cleanup(error));
            }
            return Err(HostProcessError::Cancelled);
        }

        if started.elapsed() >= control.timeout {
            let cleanup = cleanup_child_tree(&mut child);
            let (stdout, stdout_truncated) = join_output_reader(stdout_reader, "stdout")?;
            let (stderr, stderr_truncated) = join_output_reader(stderr_reader, "stderr")?;
            if let Err(error) = cleanup {
                return Err(HostProcessError::Cleanup(error));
            }
            return Err(HostProcessError::Timeout {
                stdout,
                stderr,
                stdout_truncated,
                stderr_truncated,
            });
        }

        thread::sleep(Duration::from_millis(25));
    }
}

fn is_cancelled(cancellation: Option<&AtomicBool>) -> bool {
    cancellation.is_some_and(|cancellation| cancellation.load(Ordering::Acquire))
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
fn find_command_with_login_shell(command_name: &str) -> Option<PathBuf> {
    if !command_name
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b'+'))
    {
        return None;
    }
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

fn read_capped_and_drain<R: Read>(mut reader: R, cap: usize) -> Result<(Vec<u8>, bool), String> {
    let mut output = Vec::with_capacity(cap.min(8192));
    let mut buffer = [0_u8; 8192];
    let mut truncated = false;
    loop {
        let read = reader
            .read(&mut buffer)
            .map_err(|error| error.to_string())?;
        if read == 0 {
            break;
        }
        let remaining = cap.saturating_sub(output.len());
        let retained = remaining.min(read);
        output.extend_from_slice(&buffer[..retained]);
        truncated |= retained < read;
    }
    Ok((output, truncated))
}

fn join_output_reader(
    reader: thread::JoinHandle<Result<(Vec<u8>, bool), String>>,
    stream: &str,
) -> Result<(Vec<u8>, bool), HostProcessError> {
    reader
        .join()
        .map_err(|_| HostProcessError::Output(format!("process {stream} reader panicked")))?
        .map_err(HostProcessError::Output)
}

fn cleanup_child_tree(child: &mut std::process::Child) -> Result<(), String> {
    let signal_error = signal_process_tree(child.id(), HostProcessSignal::Kill).err();
    let kill_error = if signal_error.is_some() {
        child.kill().err().map(|error| error.to_string())
    } else {
        None
    };
    let wait_error = child.wait().err().map(|error| error.to_string());
    if kill_error.is_none() && wait_error.is_none() {
        return Ok(());
    }
    Err(format!(
        "process cleanup failed: signal={:?}, kill={:?}, wait={:?}",
        signal_error, kill_error, wait_error
    ))
}

#[cfg(unix)]
pub(crate) fn configure_process_tree(command: &mut Command) {
    use std::os::unix::process::CommandExt;

    command.process_group(0);
}

#[cfg(windows)]
pub(crate) fn configure_process_tree(_command: &mut Command) {}

#[cfg(unix)]
pub(crate) fn signal_process_tree(
    process_group_id: u32,
    signal: HostProcessSignal,
) -> Result<(), String> {
    let process_group_id = libc::pid_t::try_from(process_group_id)
        .map_err(|_| "process group id is outside the platform range".to_string())?;
    if process_group_id <= 0 {
        return Err("process group id must be positive".to_string());
    }

    let signal = match signal {
        HostProcessSignal::Terminate => libc::SIGTERM,
        HostProcessSignal::Kill => libc::SIGKILL,
    };
    // SAFETY: managed children are spawned into a dedicated group whose id is
    // recorded from the direct child pid. A negative pid signals that group.
    if unsafe { libc::kill(-process_group_id, signal) } == 0 {
        return Ok(());
    }

    let error = std::io::Error::last_os_error();
    if error.raw_os_error() == Some(libc::ESRCH) {
        return Ok(());
    }
    Err(format!("failed to signal process group: {error}"))
}

#[cfg(windows)]
pub(crate) fn signal_process_tree(
    process_id: u32,
    signal: HostProcessSignal,
) -> Result<(), String> {
    use std::os::windows::process::CommandExt;

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    if process_id == 0 {
        return Err("process id must be positive".to_string());
    }

    let mut command = Command::new("taskkill");
    command.args(["/PID", &process_id.to_string(), "/T"]);
    if signal == HostProcessSignal::Kill {
        command.arg("/F");
    }
    let output = command
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .map_err(|error| format!("failed to launch taskkill: {error}"))?;
    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        if stderr.contains("not found")
            || stderr.contains("PID")
            || stdout.contains("not found")
            || stdout.contains("PID")
            || output.status.code() == Some(128)
            || output.status.code() == Some(1)
        {
            Ok(())
        } else {
            Err(format!(
                "taskkill exited with status {}: {stderr}",
                output.status
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        env,
        io::{self, Write},
        process::Command,
        time::{Duration, Instant},
    };

    #[test]
    fn process_fixture() {
        match env::var("ASSETIWEAVE_HOST_PROCESS_FIXTURE").as_deref() {
            Ok("large-output") => {
                io::stdout().write_all(&vec![b'x'; 256 * 1024]).unwrap();
            }
            Ok("timeout") => std::thread::sleep(Duration::from_secs(5)),
            #[cfg(unix)]
            Ok("launcher-exits") => {
                let _ = Command::new("sh")
                    .args(["-c", "sleep 5"])
                    .spawn()
                    .expect("spawn inherited-pipe descendant");
            }
            _ => {}
        }
    }

    #[test]
    fn drains_large_output_while_the_process_is_running() {
        let mut command = fixture_command("large-output");

        let output =
            run_command_with_timeout(&mut command, Duration::from_secs(5), 64 * 1024, 64 * 1024)
                .expect("large-output fixture should exit");

        assert!(output.status.success());
        assert_eq!(output.stdout.len(), 64 * 1024);
        assert!(output.stdout_truncated);
    }

    #[test]
    fn terminates_and_reaps_processes_after_timeout() {
        let mut command = fixture_command("timeout");
        let started = Instant::now();

        let error = run_command_with_timeout(
            &mut command,
            Duration::from_millis(100),
            64 * 1024,
            64 * 1024,
        )
        .expect_err("timeout fixture should be terminated");

        assert!(matches!(error, HostProcessError::Timeout { .. }));
        assert!(started.elapsed() < Duration::from_secs(3));
    }

    #[test]
    #[cfg(unix)]
    fn normal_exit_reaps_descendants_before_joining_output_readers() {
        let mut command = fixture_command("launcher-exits");
        let started = Instant::now();

        let output =
            run_command_with_timeout(&mut command, Duration::from_secs(2), 64 * 1024, 64 * 1024)
                .expect("launcher exit should not wait on inherited pipes");

        assert!(output.status.success());
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn async_runner_shares_bounded_output_and_reports_elapsed_time() {
        let output = run_host_command(
            HostCommandSpec {
                program: env::current_exe().expect("resolve test binary"),
                args: vec![
                    "--exact".to_string(),
                    "backend::host_process::tests::process_fixture".to_string(),
                    "--nocapture".to_string(),
                ],
                env: vec![(
                    "ASSETIWEAVE_HOST_PROCESS_FIXTURE".to_string(),
                    "large-output".to_string(),
                )],
                working_dir: None,
                stdin: HostInput::Null,
                timeout: Duration::from_secs(5),
                stdout_limit: 32 * 1024,
                stderr_limit: 32 * 1024,
            },
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
            HostCommandSpec {
                program: env::current_exe().expect("resolve test binary"),
                args: vec![
                    "--exact".to_string(),
                    "backend::host_process::tests::process_fixture".to_string(),
                    "--nocapture".to_string(),
                ],
                env: vec![(
                    "ASSETIWEAVE_HOST_PROCESS_FIXTURE".to_string(),
                    "timeout".to_string(),
                )],
                working_dir: None,
                stdin: HostInput::Null,
                timeout: Duration::from_secs(5),
                stdout_limit: 32 * 1024,
                stderr_limit: 32 * 1024,
            },
            cancellation.clone(),
        );
        tokio::pin!(task);
        tokio::time::sleep(Duration::from_millis(50)).await;
        cancellation.cancel();

        let error = task.await.expect_err("cancelled command should fail");
        assert!(matches!(error, HostProcessError::Cancelled));
    }

    #[test]
    #[cfg(unix)]
    fn process_tree_signal_is_idempotent_after_the_group_exits() {
        let mut command = fixture_command("timeout");
        configure_process_tree(&mut command);
        let mut child = command.spawn().expect("spawn process-group fixture");
        let process_group_id = child.id();

        signal_process_tree(process_group_id, HostProcessSignal::Terminate)
            .expect("first process-group terminate");
        child.wait().expect("reap process-group fixture");
        signal_process_tree(process_group_id, HostProcessSignal::Kill)
            .expect("second process-group kill is a no-op");
    }

    fn fixture_command(mode: &str) -> Command {
        let mut command = Command::new(env::current_exe().expect("resolve test binary"));
        command
            .args([
                "--exact",
                "backend::host_process::tests::process_fixture",
                "--nocapture",
            ])
            .env("ASSETIWEAVE_HOST_PROCESS_FIXTURE", mode);
        command
    }
}
