use std::{path::PathBuf, sync::Arc};

use arc_swap::ArcSwap;

use super::{Compatibility, PackageIdentity, ProbeSpec, ProcessInvocation};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct InspectedPackage {
    pub(crate) identity: PackageIdentity,
    pub(crate) compatibility: Compatibility,
    pub(crate) invocation: ProcessInvocation,
    pub(crate) availability_probe: ProbeSpec,
    pub(crate) model_discovery_probe: Option<ProbeSpec>,
    pub(crate) install_dir: PathBuf,
}

#[derive(Debug)]
pub(crate) struct RegistrySnapshot<T> {
    inner: ArcSwap<T>,
}

impl<T> RegistrySnapshot<T> {
    pub(crate) fn new(value: T) -> Self {
        Self {
            inner: ArcSwap::from_pointee(value),
        }
    }

    pub(crate) fn from_arc(value: Arc<T>) -> Self {
        Self {
            inner: ArcSwap::from(value),
        }
    }

    pub(crate) fn load(&self) -> Arc<T> {
        self.inner.load_full()
    }

    pub(crate) fn replace(&self, value: T) {
        self.inner.store(Arc::new(value));
    }
}
