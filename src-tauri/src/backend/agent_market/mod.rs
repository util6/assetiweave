//! Agent Market domain: curated catalog, installations and lifecycle contracts.
//!
//! This module intentionally contains declarative data and orchestration seams only.
//! It does not load arbitrary code from a catalog item into the application process.

mod cache;
mod catalog;
mod distribution;
mod installers;
mod lifecycle;
mod migration;
mod repository;
mod runtime;

pub(crate) mod types;

pub(crate) use cache::{CatalogCache, CatalogRefreshOutcome};
pub(crate) use distribution::{
    DistributionSelectionContext, DistributionSelector, SystemObservation,
};
pub(crate) use installers::system::SystemInstaller;
pub(crate) use installers::InstallContext;
pub(crate) use installers::Installer;
pub(crate) use lifecycle::{
    default_runtime_root, is_safe_managed_install_path, AgentLifecycleService,
};
pub(crate) use migration::migrate_legacy_assignments;
pub(crate) use repository::AgentInstallationRepository;
pub(crate) use runtime::AgentRuntimeManager;
