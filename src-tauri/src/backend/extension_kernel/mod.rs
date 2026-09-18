//! Shared primitives for installable extensions.
//!
//! The kernel owns identity, compatibility, trust gates, process invocation,
//! probing, snapshots, and lifecycle coordination. Conversation and agent
//! manifests remain domain-owned and are intentionally opaque here. A domain
//! package system only supplies kind and inspection; lifecycle side effects
//! stay in the owning domain workflow.

mod error;
mod identity;
mod launcher;
mod lifecycle;
mod registry;
mod trust;

#[allow(unused_imports)]
pub(crate) use error::ExtensionError;
#[allow(unused_imports)]
pub(crate) use identity::{Compatibility, PackageIdentity, PackageKind};
#[allow(unused_imports)]
pub(crate) use launcher::{
    EnvEntry, ExtensionLauncher, InvocationLimits, InvocationResult, ProbeKind, ProbeResult,
    ProbeSpec, ProcessInvocation, RuntimeProgramKind,
};
#[allow(unused_imports)]
pub(crate) use lifecycle::{
    LifecycleOp, LifecycleRequestKey, LifecycleReservationOutcome, LifecycleTaskCoordinator,
    ResourceKey,
};
#[allow(unused_imports)]
pub(crate) use registry::{InspectedPackage, RegistrySnapshot};
#[allow(unused_imports)]
pub(crate) use trust::TrustGate;

use std::path::Path;

use super::runtime::{AppError, WireError};

/// Domain-specific package interpretation stays outside the kernel.
pub(crate) trait DomainPackageSystem: Send + Sync {
    fn kind(&self) -> PackageKind;
    fn inspect(&self, dir: &Path) -> Result<InspectedPackage, ExtensionError>;
}

impl From<ExtensionError> for AppError {
    fn from(error: ExtensionError) -> Self {
        let view = WireError {
            code: error.code().to_string(),
            message: error.public_message(),
            retryable: error.retryable(),
            details: error.details(),
        };
        Self::from(view)
    }
}

#[cfg(test)]
#[path = "extension_kernel_tests.rs"]
mod tests;
