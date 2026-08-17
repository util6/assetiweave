use std::{
    path::{Path, PathBuf},
    sync::atomic::AtomicBool,
    time::Duration,
};

use uuid::Uuid;

use super::types::LifecycleTaskPhase;
use super::types::{Distribution, MaterializedRuntime};

pub(crate) mod binary;
pub(crate) mod npx;
pub(crate) mod system;
pub(crate) mod uvx;

pub(crate) const MAX_BINARY_BYTES: u64 = 512 * 1024 * 1024;
pub(crate) const MAX_UNPACKED_BYTES: u64 = 1024 * 1024 * 1024;
pub(crate) const MAX_FILE_COUNT: usize = 20_000;

#[derive(Clone)]
pub(crate) struct InstallContext {
    pub(crate) staging_dir: PathBuf,
    pub(crate) installation_id: String,
    pub(crate) agent_version: String,
    pub(crate) cancellation: Option<std::sync::Arc<AtomicBool>>,
    pub(crate) timeout: Duration,
    pub(crate) phase_sink: Option<std::sync::Arc<dyn Fn(LifecycleTaskPhase) + Send + Sync>>,
}

impl InstallContext {
    pub(crate) fn new(staging_dir: PathBuf, agent_version: impl Into<String>) -> Self {
        Self {
            staging_dir,
            installation_id: Uuid::new_v4().to_string(),
            agent_version: agent_version.into(),
            cancellation: None,
            timeout: Duration::from_secs(10 * 60),
            phase_sink: None,
        }
    }

    pub(crate) fn report_phase(&self, phase: LifecycleTaskPhase) {
        if let Some(sink) = self.phase_sink.as_ref() {
            sink(phase);
        }
    }
}

pub(crate) trait Installer: Send + Sync {
    fn materialize(
        &self,
        distribution: &Distribution,
        context: &InstallContext,
    ) -> Result<MaterializedRuntime, InstallError>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum InstallError {
    Unsupported(String),
    RuntimeMissing(String),
    Spawn(String),
    Failed(String),
    Cancelled,
    Timeout,
    IntegrityMismatch,
    ArchiveInvalid(String),
}

impl std::fmt::Display for InstallError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unsupported(message)
            | Self::RuntimeMissing(message)
            | Self::Spawn(message)
            | Self::Failed(message)
            | Self::ArchiveInvalid(message) => formatter.write_str(message),
            Self::Cancelled => formatter.write_str("installation was cancelled"),
            Self::Timeout => formatter.write_str("installation timed out"),
            Self::IntegrityMismatch => {
                formatter.write_str("artifact integrity verification failed")
            }
        }
    }
}

impl std::error::Error for InstallError {}

pub(crate) fn ensure_staging_root(path: &Path) -> Result<(), InstallError> {
    std::fs::create_dir_all(path).map_err(|error| InstallError::Failed(error.to_string()))
}

pub(crate) fn ensure_inside(root: &Path, candidate: &Path) -> Result<PathBuf, InstallError> {
    let root = root
        .canonicalize()
        .map_err(|error| InstallError::Failed(error.to_string()))?;
    let candidate = if candidate.exists() {
        candidate
            .canonicalize()
            .map_err(|error| InstallError::Failed(error.to_string()))?
    } else {
        candidate.to_path_buf()
    };
    if !candidate.starts_with(&root) {
        return Err(InstallError::ArchiveInvalid(
            "resolved path escapes staging root".to_string(),
        ));
    }
    Ok(candidate)
}

pub(crate) fn is_cancelled(context: &InstallContext) -> bool {
    context
        .cancellation
        .as_ref()
        .is_some_and(|flag| flag.load(std::sync::atomic::Ordering::SeqCst))
}

pub(crate) fn run_host_command(
    command: &mut std::process::Command,
    context: &InstallContext,
    stdout_cap: usize,
    stderr_cap: usize,
) -> Result<crate::backend::host_process::HostProcessOutput, InstallError> {
    crate::backend::host_process::run_command_with_control(
        command,
        crate::backend::host_process::HostProcessControl {
            timeout: context.timeout,
            stdout_cap,
            stderr_cap,
            cancellation: context.cancellation.as_deref(),
        },
    )
    .map_err(|error| match error {
        crate::backend::host_process::HostProcessError::Cancelled { .. } => InstallError::Cancelled,
        crate::backend::host_process::HostProcessError::Timeout { .. } => InstallError::Timeout,
        other => InstallError::Spawn(format!("{other:?}")),
    })
}

pub(crate) fn runtime_from_local(
    context: &InstallContext,
    program: PathBuf,
    args: Vec<String>,
    integrity: Option<serde_json::Value>,
) -> Result<MaterializedRuntime, InstallError> {
    if is_cancelled(context) {
        return Err(InstallError::Cancelled);
    }
    let program = ensure_inside(&context.staging_dir, &program)?;
    if !program.is_file() {
        return Err(InstallError::ArchiveInvalid(
            "resolved program is not a regular file".to_string(),
        ));
    }
    Ok(MaterializedRuntime {
        installation_id: context.installation_id.clone(),
        ownership: super::types::Ownership::Managed,
        install_dir: Some(context.staging_dir.clone()),
        resolved_program: program,
        args,
        env: Vec::new(),
        integrity,
        version: context.agent_version.clone(),
    })
}
