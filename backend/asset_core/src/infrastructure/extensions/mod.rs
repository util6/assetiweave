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
pub use error::ExtensionError;
#[allow(unused_imports)]
pub use identity::{Compatibility, PackageIdentity, PackageKind};
#[allow(unused_imports)]
pub use launcher::{
    EnvEntry, ExtensionLauncher, InvocationLimits, InvocationResult, ProbeKind, ProbeResult,
    ProbeSpec, ProcessInvocation, RuntimeProgramKind,
};
#[allow(unused_imports)]
pub use lifecycle::{
    LifecycleOp, LifecycleRequestKey, LifecycleReservationOutcome, LifecycleTaskCoordinator,
    ResourceKey,
};
#[allow(unused_imports)]
pub use registry::{InspectedPackage, RegistrySnapshot};
#[allow(unused_imports)]
pub use trust::TrustGate;

use std::path::Path;

/// Domain-specific package interpretation stays outside the kernel.
pub trait DomainPackageSystem: Send + Sync {
    fn kind(&self) -> PackageKind;
    fn inspect(&self, dir: &Path) -> Result<InspectedPackage, ExtensionError>;
}

pub mod installers;
pub mod layout;
pub mod manifest;
pub mod upstream_probe;

pub use layout::{
    cleanup_runtime_directories, default_runtime_root, ensure_runtime_root,
    is_safe_managed_install_path, STAGING_RETENTION,
};
pub use manifest::*;
pub use upstream_probe::*;

#[cfg(test)]
#[path = "extensions_tests.rs"]
mod tests;
