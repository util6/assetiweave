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
#[cfg(any(test, windows))]
const WINDOWS_CREATE_NO_WINDOW: u32 = 0x0800_0000;

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

#[derive(Clone, Copy)]
pub(crate) enum HostCancellation<'a> {
    Atomic(&'a std::sync::atomic::AtomicBool),
    Token(&'a tokio_util::sync::CancellationToken),
}

impl HostCancellation<'_> {
    pub(crate) fn is_cancelled(self) -> bool {
        match self {
            Self::Atomic(flag) => flag.load(std::sync::atomic::Ordering::Acquire),
            Self::Token(token) => token.is_cancelled(),
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) struct HostProcessControl<'a> {
    pub(crate) timeout: Duration,
    pub(crate) stdout_cap: usize,
    pub(crate) stderr_cap: usize,
    pub(crate) cancellation: Option<HostCancellation<'a>>,
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

    run_host_command_with_cancellation(spec, cancellation).map(|output| HostProcessOutput {
        status: output.status,
        stdout: output.stdout,
        stderr: output.stderr,
        stdout_truncated: output.stdout_truncated,
        stderr_truncated: output.stderr_truncated,
    })
}

pub(crate) fn run_host_command_with_cancellation(
    spec: HostCommandSpec,
    cancellation: Option<&tokio_util::sync::CancellationToken>,
) -> Result<HostCommandOutput, HostProcessError> {
    let mut command = build_host_command(&spec)?;
    let started = Instant::now();
    let output = run_command_with_control_and_input(
        &mut command,
        HostProcessControl {
            timeout: spec.timeout,
            stdout_cap: spec.stdout_limit,
            stderr_cap: spec.stderr_limit,
            cancellation: cancellation.map(HostCancellation::Token),
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

pub(crate) fn run_host_command_blocking(
    spec: HostCommandSpec,
) -> Result<HostCommandOutput, HostProcessError> {
    run_host_command_with_cancellation(spec, None)
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
            cancellation: cancellation.map(HostCancellation::Atomic),
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
    let worker_cancellation = cancellation.clone();
    let join = tokio::task::spawn_blocking(move || {
        let mut command = build_host_command(&spec)?;
        let started = Instant::now();
        let output = run_command_with_control_and_input(
            &mut command,
            HostProcessControl {
                timeout: spec.timeout,
                stdout_cap: spec.stdout_limit,
                stderr_cap: spec.stderr_limit,
                cancellation: Some(HostCancellation::Token(&worker_cancellation)),
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

fn is_cancelled(cancellation: Option<HostCancellation<'_>>) -> bool {
    cancellation.is_some_and(HostCancellation::is_cancelled)
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
pub(crate) fn configure_process_tree(command: &mut Command) {
    configure_background_process(command);
}

#[cfg(windows)]
pub(crate) fn configure_background_process(command: &mut Command) {
    use std::os::windows::process::CommandExt;

    command.creation_flags(windows_background_process_creation_flags());
}

#[cfg(not(windows))]
pub(crate) fn configure_background_process(_command: &mut Command) {}

#[cfg(any(test, windows))]
fn windows_background_process_creation_flags() -> u32 {
    WINDOWS_CREATE_NO_WINDOW
}

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
    if process_id == 0 {
        return Err("process id must be positive".to_string());
    }

    let mut command = Command::new("taskkill");
    command.args(["/PID", &process_id.to_string(), "/T"]);
    if signal == HostProcessSignal::Kill {
        command.arg("/F");
    }
    configure_background_process(&mut command);
    let output = command
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
            #[cfg(windows)]
            Ok("launcher-exits") => {
                let _ = Command::new("cmd")
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
                std::thread::sleep(Duration::from_secs(10));
            }
            _ => {}
        }
    }

    #[test]
    fn windows_background_processes_request_no_console_window() {
        assert_eq!(
            windows_background_process_creation_flags() & 0x0800_0000,
            0x0800_0000
        );
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

    #[test]
    fn token_cancellation_has_no_mirror_watcher_thread() {
        let source = include_str!("host_process.rs");
        assert!(!source.contains(concat!("let watcher_", "done =")));
        assert!(!source.contains(concat!("let watcher_", "token =")));
    }

    #[test]
    fn token_view_observes_cancellation_without_copying_state() {
        let token = tokio_util::sync::CancellationToken::new();
        let view = HostCancellation::Token(&token);
        assert!(!view.is_cancelled());
        token.cancel();
        assert!(view.is_cancelled());
    }

    #[test]
    fn sync_runner_cancels_and_reaps_the_process_tree() {
        let cancellation = tokio_util::sync::CancellationToken::new();
        let cancellation_clone = cancellation.clone();
        let cancel_handle = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(50));
            cancellation_clone.cancel();
        });

        let error = run_host_command_with_cancellation(
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
            Some(&cancellation),
        )
        .expect_err("cancelled sync command should fail");

        cancel_handle.join().unwrap();
        assert!(matches!(error, HostProcessError::Cancelled));
    }

    #[test]
    fn pre_cancelled_command_does_not_spawn() {
        let cancellation = tokio_util::sync::CancellationToken::new();
        cancellation.cancel();

        let error = run_program_with_cancellation(
            &env::current_exe().expect("resolve test binary"),
            &[
                "--exact".to_string(),
                "backend::host_process::tests::process_fixture".to_string(),
                "--nocapture".to_string(),
            ],
            None,
            Duration::from_secs(5),
            32 * 1024,
            32 * 1024,
            Some(&cancellation),
        )
        .expect_err("pre-cancelled command should fail immediately");

        assert!(matches!(error, HostProcessError::Cancelled));
    }

    // =========================================================================
    // C-PROCESS-01 & B2-P01: Contract Tests for process-wrap 10.0.0 and which 8.0.6
    // =========================================================================

    #[cfg(windows)]
    use process_wrap::tokio::JobObject;
    #[cfg(unix)]
    use process_wrap::tokio::ProcessGroup;
    use process_wrap::tokio::{CommandWrap, KillOnDrop};
    use tokio::io::AsyncReadExt;

    /// C-PROCESS-01 Canonical Wrapper Combination & Child Methods (Recorded for B2-P02 consumption):
    ///
    /// 1. Construction:
    ///    `let mut wrap = process_wrap::tokio::CommandWrap::from(tokio_command);`
    /// 2. Platform Process Grouping:
    ///    - Unix: `wrap.wrap(process_wrap::tokio::ProcessGroup::leader());`
    ///    - Windows: `wrap.wrap(process_wrap::tokio::JobObject);`
    /// 3. Drop Safety:
    ///    `wrap.wrap(process_wrap::tokio::KillOnDrop);`
    /// 4. Spawning & Child Management:
    ///    - Spawn: `let mut child = wrap.spawn()?;` (returns Box<dyn ChildWrapper>)
    ///    - Pipes: `child.stdout().take()`, `child.stderr().take()`
    ///    - Try Wait: `child.try_wait()? -> Option<ExitStatus>`
    ///    - Wait: `child.wait().await? -> ExitStatus`
    ///    - Termination: `child.start_kill()?` (sends SIGKILL to PGID on Unix, terminates job on Windows)
    ///    - Graceful Signal (Unix): `child.signal(libc::SIGTERM)?`
    fn wrap_test_command(cmd: tokio::process::Command) -> CommandWrap {
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

    async fn read_stream_capped<R: tokio::io::AsyncRead + Unpin>(
        mut reader: R,
        cap: usize,
    ) -> (Vec<u8>, bool) {
        let mut buf = Vec::new();
        let mut chunk = [0u8; 8192];
        let mut truncated = false;
        loop {
            match reader.read(&mut chunk).await {
                Ok(0) => break,
                Ok(n) => {
                    if buf.len() < cap {
                        let to_take = n.min(cap - buf.len());
                        buf.extend_from_slice(&chunk[..to_take]);
                        if to_take < n {
                            truncated = true;
                        }
                    } else {
                        truncated = true;
                    }
                }
                Err(_) => break,
            }
        }
        (buf, truncated)
    }

    #[derive(Debug)]
    struct TestWrapOutput {
        status: ExitStatus,
        stdout: Vec<u8>,
        stderr: Vec<u8>,
        stdout_truncated: bool,
        stderr_truncated: bool,
    }

    #[derive(Debug)]
    #[allow(dead_code)]
    enum TestWrapError {
        Timeout {
            stdout: Vec<u8>,
            stderr: Vec<u8>,
            stdout_truncated: bool,
            stderr_truncated: bool,
        },
        Cancelled,
        Spawn(String),
        Wait(String),
    }

    async fn run_tokio_wrap_fixture(
        mode: &str,
        timeout: Duration,
        stdout_limit: usize,
        stderr_limit: usize,
        cancellation: Option<&tokio_util::sync::CancellationToken>,
    ) -> Result<TestWrapOutput, TestWrapError> {
        if let Some(token) = cancellation {
            if token.is_cancelled() {
                return Err(TestWrapError::Cancelled);
            }
        }

        let cmd = make_tokio_fixture_command(mode);
        let mut wrap = wrap_test_command(cmd);
        let mut child = wrap
            .spawn()
            .map_err(|e| TestWrapError::Spawn(e.to_string()))?;

        let stdout_pipe = child.stdout().take();
        let stderr_pipe = child.stderr().take();

        let mut stdout_task = tokio::spawn(async move {
            match stdout_pipe {
                Some(r) => read_stream_capped(r, stdout_limit).await,
                None => (Vec::new(), false),
            }
        });
        let mut stderr_task = tokio::spawn(async move {
            match stderr_pipe {
                Some(r) => read_stream_capped(r, stderr_limit).await,
                None => (Vec::new(), false),
            }
        });

        let timeout_sleep = tokio::time::sleep(timeout);
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
                let status = res.map_err(|e| TestWrapError::Wait(e.to_string()))?;
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
                Ok(TestWrapOutput {
                    status,
                    stdout: stdout_res.0,
                    stderr: stderr_res.0,
                    stdout_truncated: stdout_res.1,
                    stderr_truncated: stderr_res.1,
                })
            }
            ExitReason::TimedOut => {
                let _ = child.start_kill();
                let _ = child.wait().await;
                let out = stdout_task.await.unwrap_or_default();
                let err = stderr_task.await.unwrap_or_default();
                Err(TestWrapError::Timeout {
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
                Err(TestWrapError::Cancelled)
            }
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn contract_process_wrap_normal_exit() {
        let output = run_tokio_wrap_fixture(
            "normal-exit",
            Duration::from_secs(5),
            32 * 1024,
            32 * 1024,
            None,
        )
        .await
        .expect("normal exit fixture should succeed");

        assert!(output.status.success());
        assert_eq!(output.status.code(), Some(0));
        let stdout_str = String::from_utf8_lossy(&output.stdout);
        let stderr_str = String::from_utf8_lossy(&output.stderr);
        assert!(
            stdout_str.contains("fixture-stdout-content"),
            "stdout must contain fixture output: {stdout_str}"
        );
        assert!(
            stderr_str.contains("fixture-stderr-content"),
            "stderr must contain fixture output: {stderr_str}"
        );
        assert!(!output.stdout_truncated);
        assert!(!output.stderr_truncated);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn contract_process_wrap_nonzero_exit() {
        let output = run_tokio_wrap_fixture(
            "nonzero-exit",
            Duration::from_secs(5),
            32 * 1024,
            32 * 1024,
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
        let output = run_tokio_wrap_fixture(
            "large-output",
            Duration::from_secs(5),
            32 * 1024,
            32 * 1024,
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
        let err = run_tokio_wrap_fixture(
            "timeout",
            Duration::from_millis(150),
            32 * 1024,
            32 * 1024,
            None,
        )
        .await
        .expect_err("timeout fixture must fail with timeout");

        assert!(matches!(err, TestWrapError::Timeout { .. }));
        assert!(
            started.elapsed() < Duration::from_secs(3),
            "timeout reap must be prompt"
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn contract_process_wrap_pre_cancel() {
        let cancellation = tokio_util::sync::CancellationToken::new();
        cancellation.cancel();

        let err = run_tokio_wrap_fixture(
            "timeout",
            Duration::from_secs(5),
            32 * 1024,
            32 * 1024,
            Some(&cancellation),
        )
        .await
        .expect_err("pre-cancelled command must fail immediately");

        assert!(matches!(err, TestWrapError::Cancelled));
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
        let err = run_tokio_wrap_fixture(
            "timeout",
            Duration::from_secs(5),
            32 * 1024,
            32 * 1024,
            Some(&cancellation),
        )
        .await
        .expect_err("mid-flight cancelled command must fail");

        assert!(matches!(err, TestWrapError::Cancelled));
        assert!(
            started.elapsed() < Duration::from_secs(3),
            "cancel must reap immediately"
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn contract_process_wrap_descendant_held_pipe() {
        let started = Instant::now();
        let output = run_tokio_wrap_fixture(
            "launcher-exits",
            Duration::from_secs(4),
            64 * 1024,
            64 * 1024,
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
    async fn contract_process_wrap_ignored_graceful_termination() {
        let cmd = make_tokio_fixture_command("ignore-term");
        let mut wrap = wrap_test_command(cmd);
        let mut child = wrap.spawn().expect("spawn ignore-term fixture");

        tokio::time::sleep(Duration::from_millis(100)).await;

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

        // 1. 当 bin_dir 在 PATH 中时，which::which_in 必须找到该可执行文件
        let custom_path = env::join_paths([&bin_dir]).expect("join paths");
        let found = which::which_in(cmd_name, Some(&custom_path), &work_dir)
            .expect("which must find executable in PATH");
        assert_eq!(found, exe_path);

        // 2. 当 PATH 不包含 bin_dir 且 cwd 为 work_dir 时，which 必须返回错误（standard lookup miss）
        let empty_path = OsString::from("");
        let miss = which::which_in(cmd_name, Some(&empty_path), &work_dir);
        assert!(
            miss.is_err(),
            "which must report error when not on PATH and not in cwd"
        );

        // 3. 验证 fallback 逻辑契约：
        // 只有在 which (标准 lookup) miss 时，现有 desktop fallback 才会被使用
        let standard_lookup = which::which_in(cmd_name, Some(&empty_path), &work_dir).ok();
        let resolved = standard_lookup.or_else(|| {
            // Desktop fallback candidates:
            let candidates = vec![exe_path.clone()];
            candidates.into_iter().find(|p| is_executable_file(p))
        });
        assert_eq!(
            resolved,
            Some(exe_path.clone()),
            "fallback is triggered on lookup miss"
        );

        // 当 standard lookup 成功时，优先使用标准结果，不执行 fallback
        let resolved_direct = which::which_in(cmd_name, Some(&custom_path), &work_dir)
            .ok()
            .or_else(|| panic!("fallback should not be reached when standard lookup succeeds"));
        assert_eq!(resolved_direct, Some(exe_path));

        let _ = fs::remove_dir_all(&root_temp);
    }
}
