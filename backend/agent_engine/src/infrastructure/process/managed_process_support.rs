use std::{
    collections::VecDeque,
    fmt,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::Duration,
};

use process_wrap::tokio::ChildWrapper;
use tokio::io::AsyncReadExt;

use crate::domain::definition::{AgentDefinition, AgentDefinitionError};

pub(super) const EXIT_WAIT_AFTER_KILL: Duration = Duration::from_secs(2);

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProcessExit {
    pub code: Option<i32>,
    pub success: bool,
    pub wait_error: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProcessTerminationReport {
    pub terminate_requested: bool,
    pub force_kill_requested: bool,
    pub exit: Option<ProcessExit>,
    pub signal_errors: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StderrTailSnapshot {
    pub bytes: Vec<u8>,
    pub truncated: bool,
    pub read_error: bool,
}

impl StderrTailSnapshot {
    pub fn lossy_text(&self) -> String {
        String::from_utf8_lossy(&self.bytes).into_owned()
    }

    pub fn formatted_diagnostic(&self) -> Option<String> {
        let text = self.lossy_text();
        let trimmed = text.trim();
        if trimmed.is_empty() {
            None
        } else if self.truncated {
            Some(format!("{trimmed} (truncated)"))
        } else {
            Some(trimmed.to_string())
        }
    }
}

pub fn augment_path_environment(explicit_path: Option<&str>) -> std::ffi::OsString {
    let mut paths: Vec<PathBuf> = if let Some(explicit) = explicit_path {
        std::env::split_paths(explicit).collect()
    } else if let Some(current_path) = std::env::var_os("PATH") {
        std::env::split_paths(&current_path).collect()
    } else {
        Vec::new()
    };

    #[cfg(not(windows))]
    {
        let standard_dirs = [
            PathBuf::from("/opt/homebrew/bin"),
            PathBuf::from("/usr/local/bin"),
            PathBuf::from("/opt/local/bin"),
            PathBuf::from("/usr/bin"),
            PathBuf::from("/bin"),
            PathBuf::from("/usr/sbin"),
            PathBuf::from("/sbin"),
        ];
        for dir in standard_dirs {
            if dir.is_dir() && !paths.contains(&dir) {
                paths.push(dir);
            }
        }
        if let Some(home) = dirs::home_dir() {
            let user_dirs = [
                home.join(".local").join("bin"),
                home.join(".cargo").join("bin"),
                home.join(".npm-global").join("bin"),
                home.join(".pnpm-global").join("bin"),
                home.join(".bun").join("bin"),
                home.join(".deno").join("bin"),
                home.join(".volta").join("bin"),
            ];
            for dir in user_dirs {
                if dir.is_dir() && !paths.contains(&dir) {
                    paths.push(dir);
                }
            }
        }
    }

    std::env::join_paths(paths).unwrap_or_else(|_| {
        explicit_path
            .map(std::ffi::OsString::from)
            .or_else(|| std::env::var_os("PATH"))
            .unwrap_or_default()
    })
}

#[derive(Clone, Eq, PartialEq)]
pub struct SafeSpawnPreview {
    program: PathBuf,
    argument_count: usize,
    environment_keys: Vec<String>,
    current_dir_set: bool,
}

impl SafeSpawnPreview {
    pub fn from_definition(definition: &AgentDefinition, current_dir: Option<&Path>) -> Self {
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
pub enum ManagedAgentProcessError {
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
