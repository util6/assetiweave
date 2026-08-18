use std::{path::PathBuf, time::Duration};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum RuntimeProgramKind {
    Node,
    Python,
    Bash,
    Executable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EnvEntry {
    pub(crate) key: String,
    pub(crate) value: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProcessInvocation {
    pub(crate) kind: RuntimeProgramKind,
    pub(crate) entry: String,
    pub(crate) args: Vec<String>,
    pub(crate) env: Vec<EnvEntry>,
    pub(crate) working_dir: Option<PathBuf>,
    pub(crate) version_req: Option<String>,
    pub(crate) immutable_install_dir: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProbeKind {
    Availability,
    ModelDiscovery,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProbeSpec {
    pub(crate) program: Option<String>,
    pub(crate) args: Vec<String>,
    pub(crate) env: Vec<EnvEntry>,
    pub(crate) timeout: Duration,
    pub(crate) output_limit: usize,
    pub(crate) kind: ProbeKind,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProbeResult {
    pub(crate) program: String,
    pub(crate) available: bool,
    pub(crate) version: Option<String>,
    pub(crate) required_version: Option<String>,
    pub(crate) error: Option<String>,
    pub(crate) hint: Option<String>,
}
