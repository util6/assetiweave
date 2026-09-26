use std::{
    fmt,
    path::Path,
    process::Stdio,
    sync::{Arc, Mutex},
    time::Duration,
};

use tokio::{
    process::{ChildStdin, ChildStdout, Command},
    sync::watch,
};

use crate::backend::infrastructure::host_process::{resolve_host_executable, wrap_tokio_command};

use super::managed_process_support::{
    cleanup_failed_spawn, drain_stderr, is_ignorable_signal_error, BoundedByteTail,
    EXIT_WAIT_AFTER_KILL,
};
pub(crate) use super::managed_process_support::{
    ManagedAgentProcessError, ProcessExit, ProcessTerminationReport, SafeSpawnPreview,
    StderrTailSnapshot,
};
use crate::backend::domain::agents::definition::AgentDefinition;

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

#[cfg(test)]
#[path = "managed_process_tests.rs"]
mod tests;
