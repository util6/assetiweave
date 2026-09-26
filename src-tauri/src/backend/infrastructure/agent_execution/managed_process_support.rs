use std::{
    collections::VecDeque,
    fmt,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};

use process_wrap::tokio::ChildWrapper;
use tokio::io::AsyncReadExt;

use crate::backend::domain::agents::definition::{AgentDefinition, AgentDefinitionError};

pub(super) const EXIT_WAIT_AFTER_KILL: Duration = Duration::from_secs(2);

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

pub(super) struct BoundedByteTail {
    bytes: VecDeque<u8>,
    cap: usize,
    truncated: bool,
    read_error: bool,
}

impl BoundedByteTail {
    pub(super) fn new(cap: usize) -> Self {
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

    pub(super) fn snapshot(&self) -> StderrTailSnapshot {
        StderrTailSnapshot {
            bytes: self.bytes.iter().copied().collect(),
            truncated: self.truncated,
            read_error: self.read_error,
        }
    }
}

pub(super) async fn drain_stderr(
    mut stderr: tokio::process::ChildStderr,
    tail: Arc<Mutex<BoundedByteTail>>,
) {
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

pub(super) async fn cleanup_failed_spawn(child: &mut Box<dyn ChildWrapper>) {
    let _ = child.start_kill();
    let _ = tokio::time::timeout(EXIT_WAIT_AFTER_KILL, child.wait()).await;
}

pub(super) fn is_ignorable_signal_error(_err: &std::io::Error) -> bool {
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
