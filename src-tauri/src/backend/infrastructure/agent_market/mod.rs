//! Agent Market infrastructure: catalog caching, installers, layout, materialize, and runtime management.

pub(crate) mod cache;
pub(crate) mod error;
pub(crate) mod installers;
pub(crate) mod layout;
pub(crate) mod manifest;
pub(crate) mod materialize;
pub(crate) mod runtime;
pub(crate) mod types;

pub(crate) use cache::{CatalogCache, CatalogRefreshOutcome};
pub(crate) use error::*;
pub(crate) use installers::{InstallContext, Installer, SystemInstaller};
pub(crate) use layout::*;
pub(crate) use manifest::*;
pub(crate) use materialize::*;
pub(crate) use runtime::AgentRuntimeManager;
pub(crate) use types::*;
