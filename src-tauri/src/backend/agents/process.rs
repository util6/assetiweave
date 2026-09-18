use std::{
    collections::VecDeque,
    fmt,
    path::{Path, PathBuf},
    process::Stdio,
    sync::{Arc, Mutex},
    time::Duration,
};

use process_wrap::tokio::ChildWrapper;
use tokio::{
    io::AsyncReadExt,
    process::{ChildStdin, ChildStdout, Command},
    sync::watch,
};

use crate::backend::host_process::{resolve_host_executable, wrap_tokio_command};

use super::types::{AgentDefinition, AgentDefinitionError};

const EXIT_WAIT_AFTER_KILL: Duration = Duration::from_secs(2);

enum ChildControlAction {
    Terminate {
        grace: Duration,
        reply: tokio::sync::oneshot::Sender<ProcessTerminationReport>,
    },
    #[cfg_attr(not(test), allow(dead_code))]
    ForceKill {
        reply: tokio::sync::oneshot::Sender<ProcessTerminationReport>,
    },
}

pub(crate) struct ManagedAgentProcess {
    process_id: u32,
    stdio: Mutex<Option<(ChildStdin, ChildStdout)>>,
    stderr_tail: Arc<Mutex<BoundedByteTail>>,
    stderr_done: watch::Receiver<bool>,
    exit: watch::Receiver<Option<ProcessExit>>,
    control_tx: tokio::sync::mpsc::Sender<ChildControlAction>,
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

        let mut wrap = wrap_tokio_command(command);
        let mut child = wrap
            .spawn()
            .map_err(|error| ManagedAgentProcessError::Spawn {
                preview,
                message: error.to_string(),
            })?;
        let process_id = match child.id() {
            Some(process_id) => process_id,
            None => {
                cleanup_failed_spawn(&mut child).await;
                return Err(ManagedAgentProcessError::MissingProcessId);
            }
        };
        let stdin = match child.stdin().take() {
            Some(stdin) => stdin,
            None => {
                cleanup_failed_spawn(&mut child).await;
                return Err(ManagedAgentProcessError::MissingStdio("stdin"));
            }
        };
        let stdout = match child.stdout().take() {
            Some(stdout) => stdout,
            None => {
                cleanup_failed_spawn(&mut child).await;
                return Err(ManagedAgentProcessError::MissingStdio("stdout"));
            }
        };
        let stderr = match child.stderr().take() {
            Some(stderr) => stderr,
            None => {
                cleanup_failed_spawn(&mut child).await;
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
        let (control_tx, mut control_rx) = tokio::sync::mpsc::channel::<ChildControlAction>(4);

        tokio::spawn(async move {
            let mut child_exited = false;
            let mut child_exit_snapshot: Option<ProcessExit> = None;

            loop {
                let action = if !child_exited {
                    tokio::select! {
                        res = child.wait() => {
                            let snapshot = match res {
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
                            child_exit_snapshot = Some(snapshot.clone());
                            let _ = exit_tx.send(Some(snapshot));
                            child_exited = true;
                            continue;
                        }
                        action = control_rx.recv() => action,
                    }
                } else {
                    control_rx.recv().await
                };

                match action {
                    None => {
                        let _ = child.start_kill();
                        break;
                    }
                    Some(ChildControlAction::Terminate { grace, reply }) => {
                        let mut signal_errors = Vec::new();
                        #[cfg(unix)]
                        {
                            if let Err(e) = child.signal(libc::SIGTERM) {
                                if !is_ignorable_signal_error(&e) {
                                    signal_errors.push(e.to_string());
                                }
                            }
                        }

                        if !child_exited && !grace.is_zero() {
                            tokio::select! {
                                res = child.wait() => {
                                    let snapshot = match res {
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
                                    child_exit_snapshot = Some(snapshot.clone());
                                    let _ = exit_tx.send(Some(snapshot));
                                    child_exited = true;
                                }
                                _ = tokio::time::sleep(grace) => {}
                            }
                        }

                        if !child_exited {
                            if let Err(e) = child.start_kill() {
                                if !is_ignorable_signal_error(&e) {
                                    signal_errors.push(e.to_string());
                                }
                            }
                        }

                        if !child_exited {
                            tokio::select! {
                                res = child.wait() => {
                                    let snapshot = match res {
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
                                    child_exit_snapshot = Some(snapshot.clone());
                                    let _ = exit_tx.send(Some(snapshot));
                                }
                                _ = tokio::time::sleep(EXIT_WAIT_AFTER_KILL) => {}
                            }
                        }

                        let _ = reply.send(ProcessTerminationReport {
                            terminate_requested: true,
                            force_kill_requested: true,
                            exit: child_exit_snapshot,
                            signal_errors,
                        });
                        break;
                    }
                    Some(ChildControlAction::ForceKill { reply }) => {
                        let mut signal_errors = Vec::new();
                        if let Err(e) = child.start_kill() {
                            if !is_ignorable_signal_error(&e) {
                                signal_errors.push(e.to_string());
                            }
                        }

                        if !child_exited {
                            tokio::select! {
                                res = child.wait() => {
                                    let snapshot = match res {
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
                                    child_exit_snapshot = Some(snapshot.clone());
                                    let _ = exit_tx.send(Some(snapshot));
                                }
                                _ = tokio::time::sleep(EXIT_WAIT_AFTER_KILL) => {}
                            }
                        }

                        let _ = reply.send(ProcessTerminationReport {
                            terminate_requested: false,
                            force_kill_requested: true,
                            exit: child_exit_snapshot,
                            signal_errors,
                        });
                        break;
                    }
                }
            }
        });

        Ok(Self {
            process_id,
            stdio: Mutex::new(Some((stdin, stdout))),
            stderr_tail,
            stderr_done,
            exit,
            control_tx,
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
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
        if self
            .control_tx
            .send(ChildControlAction::Terminate {
                grace,
                reply: reply_tx,
            })
            .await
            .is_err()
        {
            return ProcessTerminationReport {
                terminate_requested: true,
                force_kill_requested: false,
                exit: self.current_exit(),
                signal_errors: Vec::new(),
            };
        }
        reply_rx.await.unwrap_or_else(|_| ProcessTerminationReport {
            terminate_requested: true,
            force_kill_requested: true,
            exit: self.current_exit(),
            signal_errors: Vec::new(),
        })
    }

    #[cfg(test)]
    pub(crate) async fn force_kill_tree(&self) -> ProcessTerminationReport {
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();
        if self
            .control_tx
            .send(ChildControlAction::ForceKill { reply: reply_tx })
            .await
            .is_err()
        {
            return ProcessTerminationReport {
                terminate_requested: false,
                force_kill_requested: true,
                exit: self.current_exit(),
                signal_errors: Vec::new(),
            };
        }
        reply_rx.await.unwrap_or_else(|_| ProcessTerminationReport {
            terminate_requested: false,
            force_kill_requested: true,
            exit: self.current_exit(),
            signal_errors: Vec::new(),
        })
    }
}

impl fmt::Debug for ManagedAgentProcess {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ManagedAgentProcess")
            .field("process_id", &self.process_id)
            .field(
                "stdio_taken",
                &self.stdio.lock().map_or(true, |stdio| stdio.is_none()),
            )
            .field("exit", &self.current_exit())
            .finish_non_exhaustive()
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
    #[cfg(test)]
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

#[derive(Debug, thiserror::Error)]
pub(crate) enum ManagedAgentProcessError {
    #[error("invalid agent definition: {0}")]
    InvalidDefinition(#[source] AgentDefinitionError),
    #[error("{command_name} was not found on this host")]
    ExecutableNotFound { command_name: String },
    #[error("agent executable resolution worker failed")]
    ExecutableResolutionFailed,
    #[error("failed to spawn {preview:?}: {message}")]
    Spawn {
        preview: SafeSpawnPreview,
        message: String,
    },
    #[error("spawned process has no process id")]
    MissingProcessId,
    #[error("spawned process has no piped {0}")]
    MissingStdio(&'static str),
    #[error("process stdio was already taken")]
    StdioAlreadyTaken,
    #[error("process {0} state is unavailable")]
    StateUnavailable(&'static str),
}

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

async fn cleanup_failed_spawn(child: &mut Box<dyn ChildWrapper>) {
    let _ = child.start_kill();
    let _ = tokio::time::timeout(EXIT_WAIT_AFTER_KILL, child.wait()).await;
}

fn is_ignorable_signal_error(_err: &std::io::Error) -> bool {
    #[cfg(unix)]
    {
        if _err.raw_os_error() == Some(libc::ESRCH) {
            return true;
        }
    }
    #[cfg(windows)]
    {
        if matches!(_err.raw_os_error(), Some(5) | Some(87)) {
            return true;
        }
    }
    false
}

#[cfg(test)]
#[path = "process_tests.rs"]
mod tests;
