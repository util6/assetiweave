use std::{
    collections::VecDeque,
    fmt,
    path::{Path, PathBuf},
    process::Stdio,
    sync::{Arc, Mutex},
    time::Duration,
};

use tokio::{
    io::AsyncReadExt,
    process::{Child, ChildStdin, ChildStdout, Command},
    sync::watch,
};

use crate::backend::host_process::{
    configure_process_tree, resolve_host_executable, signal_process_tree, HostProcessSignal,
};

use super::types::{AgentDefinition, AgentDefinitionError};

const EXIT_WAIT_AFTER_KILL: Duration = Duration::from_secs(2);

pub(crate) struct ManagedAgentProcess {
    process_id: u32,
    process_group_id: u32,
    stdio: Mutex<Option<(ChildStdin, ChildStdout)>>,
    stderr_tail: Arc<Mutex<BoundedByteTail>>,
    stderr_done: watch::Receiver<bool>,
    exit: watch::Receiver<Option<ProcessExit>>,
}

impl ManagedAgentProcess {
    pub(crate) async fn spawn(
        definition: &AgentDefinition,
        current_dir: Option<&Path>,
        stderr_cap: usize,
    ) -> Result<Self, ManagedAgentProcessError> {
        definition
            .validate()
            .map_err(ManagedAgentProcessError::InvalidDefinition)?;
        let preview = SafeSpawnPreview::from_definition(definition, current_dir);
        let command_name = definition.command.clone();
        let program = tokio::task::spawn_blocking(move || resolve_host_executable(&command_name))
            .await
            .map_err(|_| ManagedAgentProcessError::ExecutableResolutionFailed)?
            .ok_or_else(|| ManagedAgentProcessError::ExecutableNotFound {
                command_name: definition.command.clone(),
            })?;
        let mut command = Command::new(program);
        command
            .args(&definition.args)
            .envs(
                definition
                    .env
                    .iter()
                    .map(|entry| (&entry.name, &entry.value)),
            )
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(current_dir) = current_dir {
            command.current_dir(current_dir);
        }
        configure_process_tree(command.as_std_mut());

        let mut child = command
            .spawn()
            .map_err(|error| ManagedAgentProcessError::Spawn {
                preview,
                message: error.to_string(),
            })?;
        let process_id = match child.id() {
            Some(process_id) => process_id,
            None => {
                cleanup_failed_spawn(&mut child, None).await;
                return Err(ManagedAgentProcessError::MissingProcessId);
            }
        };
        let stdin = match child.stdin.take() {
            Some(stdin) => stdin,
            None => {
                cleanup_failed_spawn(&mut child, Some(process_id)).await;
                return Err(ManagedAgentProcessError::MissingStdio("stdin"));
            }
        };
        let stdout = match child.stdout.take() {
            Some(stdout) => stdout,
            None => {
                cleanup_failed_spawn(&mut child, Some(process_id)).await;
                return Err(ManagedAgentProcessError::MissingStdio("stdout"));
            }
        };
        let stderr = match child.stderr.take() {
            Some(stderr) => stderr,
            None => {
                cleanup_failed_spawn(&mut child, Some(process_id)).await;
                return Err(ManagedAgentProcessError::MissingStdio("stderr"));
            }
        };

        let stderr_tail = Arc::new(Mutex::new(BoundedByteTail::new(stderr_cap)));
        let (stderr_done_tx, stderr_done) = watch::channel(false);
        let tail_for_reader = Arc::clone(&stderr_tail);
        tokio::spawn(async move {
            drain_stderr(stderr, tail_for_reader).await;
            let _ = stderr_done_tx.send(true);
        });

        let (exit_tx, exit) = watch::channel(None);
        tokio::spawn(async move {
            let snapshot = match child.wait().await {
                Ok(status) => ProcessExit {
                    code: status.code(),
                    success: status.success(),
                    wait_error: None,
                },
                Err(error) => ProcessExit {
                    code: None,
                    success: false,
                    wait_error: Some(error.to_string()),
                },
            };
            let _ = exit_tx.send(Some(snapshot));
        });

        Ok(Self {
            process_id,
            process_group_id: process_id,
            stdio: Mutex::new(Some((stdin, stdout))),
            stderr_tail,
            stderr_done,
            exit,
        })
    }

    pub(crate) fn process_id(&self) -> u32 {
        self.process_id
    }

    pub(crate) async fn take_stdio(
        &self,
    ) -> Result<(ChildStdin, ChildStdout), ManagedAgentProcessError> {
        self.stdio
            .lock()
            .map_err(|_| ManagedAgentProcessError::StateUnavailable("stdio"))?
            .take()
            .ok_or(ManagedAgentProcessError::StdioAlreadyTaken)
    }

    pub(crate) fn stderr_tail(&self) -> Result<StderrTailSnapshot, ManagedAgentProcessError> {
        let tail = self
            .stderr_tail
            .lock()
            .map_err(|_| ManagedAgentProcessError::StateUnavailable("stderr_tail"))?;
        Ok(tail.snapshot())
    }

    pub(crate) async fn wait_for_stderr_eof(&self, timeout: Duration) -> bool {
        if *self.stderr_done.borrow() {
            return true;
        }
        let mut stderr_done = self.stderr_done.clone();
        tokio::time::timeout(timeout, async move {
            while stderr_done.changed().await.is_ok() {
                if *stderr_done.borrow() {
                    return true;
                }
            }
            *stderr_done.borrow()
        })
        .await
        .unwrap_or(false)
    }

    pub(crate) fn current_exit(&self) -> Option<ProcessExit> {
        self.exit.borrow().clone()
    }

    pub(crate) async fn wait_for_exit(&self) -> Option<ProcessExit> {
        if let Some(exit) = self.current_exit() {
            return Some(exit);
        }
        let mut exit = self.exit.clone();
        while exit.changed().await.is_ok() {
            if let Some(snapshot) = exit.borrow().clone() {
                return Some(snapshot);
            }
        }
        let snapshot = exit.borrow().clone();
        snapshot
    }

    pub(crate) async fn terminate(&self, grace: Duration) -> ProcessTerminationReport {
        let mut signal_errors = Vec::new();
        if let Err(error) = signal_process_tree(self.process_group_id, HostProcessSignal::Terminate)
        {
            signal_errors.push(error);
        }

        let mut exit = self.current_exit();
        if exit.is_none() && !grace.is_zero() {
            exit = tokio::time::timeout(grace, self.wait_for_exit())
                .await
                .ok()
                .flatten();
        }

        // Always address the recorded group after the grace window. This also
        // cleans descendants when a launcher exits before its process tree.
        if let Err(error) = signal_process_tree(self.process_group_id, HostProcessSignal::Kill) {
            signal_errors.push(error);
        }
        if exit.is_none() {
            exit = tokio::time::timeout(EXIT_WAIT_AFTER_KILL, self.wait_for_exit())
                .await
                .ok()
                .flatten();
        }

        ProcessTerminationReport {
            terminate_requested: true,
            force_kill_requested: true,
            exit,
            signal_errors,
        }
    }

    pub(crate) async fn force_kill_tree(&self) -> ProcessTerminationReport {
        let mut signal_errors = Vec::new();
        if let Err(error) = signal_process_tree(self.process_group_id, HostProcessSignal::Kill) {
            signal_errors.push(error);
        }
        let exit = tokio::time::timeout(EXIT_WAIT_AFTER_KILL, self.wait_for_exit())
            .await
            .ok()
            .flatten();
        ProcessTerminationReport {
            terminate_requested: false,
            force_kill_requested: true,
            exit,
            signal_errors,
        }
    }
}

impl fmt::Debug for ManagedAgentProcess {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ManagedAgentProcess")
            .field("process_id", &self.process_id)
            .field("process_group_id", &self.process_group_id)
            .field(
                "stdio_taken",
                &self.stdio.lock().map_or(true, |stdio| stdio.is_none()),
            )
            .field("exit", &self.current_exit())
            .finish_non_exhaustive()
    }
}

impl Drop for ManagedAgentProcess {
    fn drop(&mut self) {
        if self.current_exit().is_none() {
            let _ = signal_process_tree(self.process_group_id, HostProcessSignal::Kill);
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ProcessExit {
    pub(crate) code: Option<i32>,
    pub(crate) success: bool,
    pub(crate) wait_error: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ProcessTerminationReport {
    pub(crate) terminate_requested: bool,
    pub(crate) force_kill_requested: bool,
    pub(crate) exit: Option<ProcessExit>,
    pub(crate) signal_errors: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct StderrTailSnapshot {
    pub(crate) bytes: Vec<u8>,
    pub(crate) truncated: bool,
    pub(crate) read_error: bool,
}

impl StderrTailSnapshot {
    pub(crate) fn lossy_text(&self) -> String {
        String::from_utf8_lossy(&self.bytes).into_owned()
    }
}

#[derive(Clone, Eq, PartialEq)]
pub(crate) struct SafeSpawnPreview {
    program: PathBuf,
    argument_count: usize,
    environment_keys: Vec<String>,
    current_dir_set: bool,
}

impl SafeSpawnPreview {
    pub(crate) fn from_definition(
        definition: &AgentDefinition,
        current_dir: Option<&Path>,
    ) -> Self {
        Self {
            program: PathBuf::from(&definition.command),
            argument_count: definition.args.len(),
            environment_keys: definition
                .env
                .iter()
                .map(|entry| entry.name.clone())
                .collect(),
            current_dir_set: current_dir.is_some(),
        }
    }
}

impl fmt::Debug for SafeSpawnPreview {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SafeSpawnPreview")
            .field("program", &self.program)
            .field("argument_count", &self.argument_count)
            .field("environment_keys", &self.environment_keys)
            .field("current_dir_set", &self.current_dir_set)
            .finish()
    }
}

#[derive(Debug)]
pub(crate) enum ManagedAgentProcessError {
    InvalidDefinition(AgentDefinitionError),
    ExecutableNotFound {
        command_name: String,
    },
    ExecutableResolutionFailed,
    Spawn {
        preview: SafeSpawnPreview,
        message: String,
    },
    MissingProcessId,
    MissingStdio(&'static str),
    StdioAlreadyTaken,
    StateUnavailable(&'static str),
}

impl fmt::Display for ManagedAgentProcessError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidDefinition(error) => {
                write!(formatter, "invalid agent definition: {error}")
            }
            Self::ExecutableNotFound { command_name } => {
                write!(formatter, "{command_name} was not found on this host")
            }
            Self::ExecutableResolutionFailed => {
                formatter.write_str("agent executable resolution worker failed")
            }
            Self::Spawn { preview, message } => {
                write!(formatter, "failed to spawn {preview:?}: {message}")
            }
            Self::MissingProcessId => formatter.write_str("spawned process has no process id"),
            Self::MissingStdio(stream) => {
                write!(formatter, "spawned process has no piped {stream}")
            }
            Self::StdioAlreadyTaken => formatter.write_str("process stdio was already taken"),
            Self::StateUnavailable(state) => {
                write!(formatter, "process {state} state is unavailable")
            }
        }
    }
}

impl std::error::Error for ManagedAgentProcessError {}

struct BoundedByteTail {
    bytes: VecDeque<u8>,
    cap: usize,
    truncated: bool,
    read_error: bool,
}

impl BoundedByteTail {
    fn new(cap: usize) -> Self {
        Self {
            bytes: VecDeque::with_capacity(cap.min(8192)),
            cap,
            truncated: false,
            read_error: false,
        }
    }

    fn push(&mut self, chunk: &[u8]) {
        if chunk.is_empty() {
            return;
        }
        if self.cap == 0 {
            self.truncated = true;
            return;
        }
        if chunk.len() >= self.cap {
            self.bytes.clear();
            self.bytes.extend(
                chunk[chunk.len().saturating_sub(self.cap)..]
                    .iter()
                    .copied(),
            );
            self.truncated = true;
            return;
        }

        let overflow = self
            .bytes
            .len()
            .saturating_add(chunk.len())
            .saturating_sub(self.cap);
        if overflow > 0 {
            self.bytes.drain(..overflow);
            self.truncated = true;
        }
        self.bytes.extend(chunk.iter().copied());
    }

    fn snapshot(&self) -> StderrTailSnapshot {
        StderrTailSnapshot {
            bytes: self.bytes.iter().copied().collect(),
            truncated: self.truncated,
            read_error: self.read_error,
        }
    }
}

async fn drain_stderr(mut stderr: tokio::process::ChildStderr, tail: Arc<Mutex<BoundedByteTail>>) {
    let mut buffer = [0_u8; 8192];
    loop {
        match stderr.read(&mut buffer).await {
            Ok(0) => break,
            Ok(read) => {
                if let Ok(mut tail) = tail.lock() {
                    tail.push(&buffer[..read]);
                } else {
                    break;
                }
            }
            Err(_) => {
                if let Ok(mut tail) = tail.lock() {
                    tail.read_error = true;
                }
                break;
            }
        }
    }
}

async fn cleanup_failed_spawn(child: &mut Child, process_id: Option<u32>) {
    if let Some(process_id) = process_id {
        let _ = signal_process_tree(process_id, HostProcessSignal::Kill);
    }
    let _ = child.start_kill();
    let _ = tokio::time::timeout(EXIT_WAIT_AFTER_KILL, child.wait()).await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::agents::types::{
        AgentDefinition, AgentEnvEntry, AgentId, AgentProtocol, DeclaredAgentCapabilities,
    };
    use std::{
        env,
        io::{Read, Write},
        time::Duration,
    };
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[test]
    fn process_fixture() {
        match env::var("ASSETIWEAVE_MANAGED_PROCESS_FIXTURE").as_deref() {
            Ok("large-stderr") => {
                std::io::stderr().write_all(&vec![b'e'; 64 * 1024]).unwrap();
            }
            Ok("broken-stderr") => {
                std::io::stderr().write_all(&[0xff, b'a', 0xfe]).unwrap();
            }
            Ok("echo") => {
                let mut input = String::new();
                std::io::stdin().read_to_string(&mut input).unwrap();
                write!(std::io::stdout(), "echo:{input}").unwrap();
            }
            Ok("env-overlay") => {
                write!(
                    std::io::stdout(),
                    "overlay:{}",
                    env::var("ASSETIWEAVE_TEST_OVERLAY").unwrap_or_default()
                )
                .unwrap();
            }
            #[cfg(unix)]
            Ok("ignore-term") => {
                // SAFETY: this fixture intentionally ignores SIGTERM so the
                // parent test can prove the SIGKILL fallback converges.
                unsafe {
                    libc::signal(libc::SIGTERM, libc::SIG_IGN);
                }
                std::fs::write(
                    env::var("ASSETIWEAVE_MANAGED_PROCESS_PID_FILE").unwrap(),
                    std::process::id().to_string(),
                )
                .expect("write ignore-term readiness pid");
                std::thread::sleep(Duration::from_secs(5));
            }
            Ok("grandchild") | Ok("launcher-exits") => {
                let mode = env::var("ASSETIWEAVE_MANAGED_PROCESS_FIXTURE").unwrap();
                let mut child = std::process::Command::new(
                    env::current_exe().expect("resolve grandchild fixture binary"),
                )
                .args([
                    "--exact",
                    "backend::agents::process::tests::process_fixture",
                    "--nocapture",
                ])
                .env("ASSETIWEAVE_MANAGED_PROCESS_FIXTURE", "grandchild-leaf")
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .expect("spawn grandchild fixture");
                std::fs::write(
                    env::var("ASSETIWEAVE_MANAGED_PROCESS_PID_FILE").unwrap(),
                    child.id().to_string(),
                )
                .expect("write grandchild pid");
                if mode == "grandchild" {
                    std::thread::sleep(Duration::from_secs(5));
                    let _ = child.kill();
                    let _ = child.wait();
                }
            }
            Ok("grandchild-leaf") => std::thread::sleep(Duration::from_secs(5)),
            Ok("sleep") => std::thread::sleep(Duration::from_secs(5)),
            _ => {}
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn first_stdio_take_supports_bidirectional_io() {
        let process = spawn_fixture("echo", 1024).await;
        let (mut stdin, mut stdout) = process.take_stdio().await.expect("first stdio take");

        stdin.write_all(b"PING").await.expect("write child stdin");
        stdin.shutdown().await.expect("close child stdin");
        drop(stdin);
        let mut output = String::new();
        tokio::time::timeout(Duration::from_secs(3), stdout.read_to_string(&mut output))
            .await
            .expect("stdout read in time")
            .expect("read child stdout");

        assert!(output.contains("echo:PING"));
        assert!(process.wait_for_exit().await.is_some());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn stdio_can_only_be_taken_once() {
        let process = spawn_fixture("sleep", 1024).await;

        let first = process.take_stdio().await;
        let second = process.take_stdio().await;

        assert!(first.is_ok());
        assert!(matches!(
            second,
            Err(ManagedAgentProcessError::StdioAlreadyTaken)
        ));
        process.force_kill_tree().await;
    }

    #[tokio::test(flavor = "current_thread")]
    async fn stderr_is_drained_into_a_bounded_tail() {
        let process = spawn_fixture("large-stderr", 1024).await;
        tokio::time::timeout(Duration::from_secs(3), process.wait_for_exit())
            .await
            .expect("fixture exits in time")
            .expect("exit snapshot");
        assert!(process.wait_for_stderr_eof(Duration::from_secs(1)).await);

        let tail = process.stderr_tail().expect("stderr tail");
        assert_eq!(tail.bytes.len(), 1024);
        assert!(tail.truncated);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn broken_utf8_stderr_has_a_lossy_diagnostic() {
        let process = spawn_fixture("broken-stderr", 1024).await;
        tokio::time::timeout(Duration::from_secs(3), process.wait_for_exit())
            .await
            .expect("fixture exits in time")
            .expect("exit snapshot");
        assert!(process.wait_for_stderr_eof(Duration::from_secs(1)).await);

        let diagnostic = process.stderr_tail().unwrap().lossy_text();

        assert!(diagnostic.contains('�'));
        assert!(diagnostic.contains('a'));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn immediate_exit_is_published_with_status() {
        let process = spawn_fixture("immediate-exit", 1024).await;

        let exit = tokio::time::timeout(Duration::from_secs(3), process.wait_for_exit())
            .await
            .expect("fixture exits in time")
            .expect("exit snapshot");

        assert!(exit.success);
        assert_eq!(exit.code, Some(0));
        assert!(exit.wait_error.is_none());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn exit_is_watched_and_termination_is_idempotent() {
        let process = spawn_fixture("sleep", 1024).await;

        let first = tokio::time::timeout(
            Duration::from_secs(3),
            process.terminate(Duration::from_millis(100)),
        )
        .await
        .expect("first termination in time");
        let second = tokio::time::timeout(
            Duration::from_secs(3),
            process.terminate(Duration::from_millis(100)),
        )
        .await
        .expect("second termination in time");

        assert!(first.exit.is_some());
        assert!(second.exit.is_some());
    }

    #[tokio::test(flavor = "current_thread")]
    #[cfg(unix)]
    async fn graceful_deadline_converges_through_force_kill() {
        let process = spawn_fixture("sleep", 1024).await;

        let report = tokio::time::timeout(
            Duration::from_secs(3),
            process.terminate(Duration::from_millis(50)),
        )
        .await
        .expect("termination in time");

        assert!(report.terminate_requested);
        assert!(report.force_kill_requested);
        assert!(report.exit.is_some());
        assert!(report.signal_errors.is_empty());
    }

    #[tokio::test(flavor = "current_thread")]
    #[cfg(unix)]
    async fn sigkill_fallback_stops_a_process_that_ignores_sigterm() {
        let pid_file = temp_pid_file();
        let process = spawn_tree_fixture("ignore-term", &pid_file).await;
        let _ready_pid = read_pid_file(&pid_file).await;
        let started = std::time::Instant::now();

        let report = tokio::time::timeout(
            Duration::from_secs(3),
            process.terminate(Duration::from_millis(75)),
        )
        .await
        .expect("termination in time");

        assert!(started.elapsed() >= Duration::from_millis(60));
        assert!(report.exit.is_some());
        assert!(!report.exit.unwrap().success);
        let _ = std::fs::remove_file(pid_file);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn force_kill_stops_the_direct_child_and_grandchild() {
        let pid_file = temp_pid_file();
        let process = spawn_tree_fixture("grandchild", &pid_file).await;
        let direct_pid = process.process_id();
        let grandchild_pid = read_pid_file(&pid_file).await;

        let report = tokio::time::timeout(Duration::from_secs(3), process.force_kill_tree())
            .await
            .expect("force kill in time");

        assert!(report.exit.is_some());
        wait_until_process_is_gone(direct_pid).await;
        wait_until_process_is_gone(grandchild_pid).await;
        let _ = std::fs::remove_file(pid_file);
    }

    #[tokio::test(flavor = "current_thread")]
    #[cfg(unix)]
    async fn recorded_group_is_cleaned_after_the_launcher_exits() {
        let pid_file = temp_pid_file();
        let process = spawn_tree_fixture("launcher-exits", &pid_file).await;
        let grandchild_pid = read_pid_file(&pid_file).await;
        tokio::time::timeout(Duration::from_secs(3), process.wait_for_exit())
            .await
            .expect("launcher exits in time")
            .expect("launcher exit snapshot");
        assert!(process_exists(grandchild_pid));

        tokio::time::timeout(
            Duration::from_secs(3),
            process.terminate(Duration::from_millis(50)),
        )
        .await
        .expect("tree cleanup in time");

        wait_until_process_is_gone(grandchild_pid).await;
        let _ = std::fs::remove_file(pid_file);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn environment_overlay_is_applied_without_entering_the_preview() {
        let mut definition = fixture_definition("env-overlay");
        definition.env.push(AgentEnvEntry::new(
            "ASSETIWEAVE_TEST_OVERLAY",
            "SECRET_OVERLAY_VALUE",
        ));
        let preview = format!("{:?}", SafeSpawnPreview::from_definition(&definition, None));
        let process = ManagedAgentProcess::spawn(&definition, None, 1024)
            .await
            .expect("spawn fixture");
        let (stdin, mut stdout) = process.take_stdio().await.expect("take stdio");
        drop(stdin);
        let mut output = String::new();
        tokio::time::timeout(Duration::from_secs(3), stdout.read_to_string(&mut output))
            .await
            .expect("stdout read in time")
            .expect("read child stdout");

        assert!(output.contains("overlay:SECRET_OVERLAY_VALUE"));
        assert!(!preview.contains("SECRET_OVERLAY_VALUE"));
    }

    #[test]
    fn spawn_preview_does_not_expose_argument_or_environment_values() {
        let mut definition = fixture_definition("SECRET_ENV_VALUE");
        definition.args.push("SECRET_ARGUMENT".to_string());

        let preview = SafeSpawnPreview::from_definition(&definition, None);
        let debug = format!("{preview:?}");

        assert!(!debug.contains("SECRET_ARGUMENT"));
        assert!(!debug.contains("SECRET_ENV_VALUE"));
        assert!(debug.contains("ASSETIWEAVE_MANAGED_PROCESS_FIXTURE"));
    }

    async fn spawn_fixture(mode: &str, stderr_cap: usize) -> ManagedAgentProcess {
        tokio::time::timeout(
            Duration::from_secs(3),
            ManagedAgentProcess::spawn(&fixture_definition(mode), None, stderr_cap),
        )
        .await
        .expect("spawn in time")
        .expect("spawn fixture")
    }

    async fn spawn_tree_fixture(mode: &str, pid_file: &Path) -> ManagedAgentProcess {
        let mut definition = fixture_definition(mode);
        definition.env.push(AgentEnvEntry::new(
            "ASSETIWEAVE_MANAGED_PROCESS_PID_FILE",
            pid_file.to_string_lossy(),
        ));
        tokio::time::timeout(
            Duration::from_secs(3),
            ManagedAgentProcess::spawn(&definition, None, 1024),
        )
        .await
        .expect("tree spawn in time")
        .expect("spawn tree fixture")
    }

    fn temp_pid_file() -> PathBuf {
        env::temp_dir().join(format!(
            "assetiweave-managed-process-{}.pid",
            uuid::Uuid::new_v4()
        ))
    }

    async fn read_pid_file(path: &Path) -> u32 {
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if let Ok(value) = std::fs::read_to_string(path) {
                    if let Ok(pid) = value.trim().parse() {
                        break pid;
                    }
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("grandchild pid file in time")
    }

    async fn wait_until_process_is_gone(process_id: u32) {
        tokio::time::timeout(Duration::from_secs(2), async move {
            while process_exists(process_id) {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("process exits in time");
    }

    #[cfg(unix)]
    fn process_exists(process_id: u32) -> bool {
        // SAFETY: signal zero performs existence/permission checking only.
        let result = unsafe { libc::kill(process_id as libc::pid_t, 0) };
        result == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
    }

    #[cfg(windows)]
    fn process_exists(process_id: u32) -> bool {
        use std::os::windows::process::CommandExt;

        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let output = std::process::Command::new("tasklist")
            .args(["/FI", &format!("PID eq {process_id}"), "/FO", "CSV", "/NH"])
            .creation_flags(CREATE_NO_WINDOW)
            .output();
        output.is_ok_and(|output| {
            String::from_utf8_lossy(&output.stdout).lines().any(|line| {
                line.split(',')
                    .nth(1)
                    .is_some_and(|pid| pid.trim().trim_matches('"') == process_id.to_string())
            })
        })
    }

    fn fixture_definition(mode: &str) -> AgentDefinition {
        AgentDefinition {
            id: AgentId::parse("fixture").unwrap(),
            installation_id: None,
            display_name: "Fixture".to_string(),
            protocol: AgentProtocol::Acp,
            command: env::current_exe()
                .expect("resolve test binary")
                .to_string_lossy()
                .into_owned(),
            args: vec![
                "--exact".to_string(),
                "backend::agents::process::tests::process_fixture".to_string(),
                "--nocapture".to_string(),
            ],
            env: vec![AgentEnvEntry::new(
                "ASSETIWEAVE_MANAGED_PROCESS_FIXTURE",
                mode,
            )],
            declared_capabilities: DeclaredAgentCapabilities::acp_text(),
            availability_probe: None,
            model_discovery: None,
        }
    }
}
