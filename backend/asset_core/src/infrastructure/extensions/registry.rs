use std::{path::PathBuf, sync::Arc};

use arc_swap::ArcSwap;

use super::{Compatibility, PackageIdentity, ProbeSpec, ProcessInvocation};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InspectedPackage {
    pub identity: PackageIdentity,
    pub compatibility: Compatibility,
    pub invocation: ProcessInvocation,
    pub availability_probe: ProbeSpec,
    pub model_discovery_probe: Option<ProbeSpec>,
    pub install_dir: PathBuf,
}

pub use agent_engine::infrastructure::RegistrySnapshot;
