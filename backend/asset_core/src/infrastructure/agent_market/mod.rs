//! Agent Market infrastructure: catalog caching, installers, layout, materialize, and runtime management.

pub mod cache;
pub mod error;
pub mod installers;
pub mod layout;
pub mod manifest;
pub mod materialize;
pub mod runtime;
pub mod types;
pub mod upstream_probe;

pub use cache::{purge_legacy_agent_market_cache_disk_files, CatalogCache, CatalogRefreshOutcome};
pub use error::*;
pub use installers::{InstallContext, Installer};
pub use layout::*;
pub use manifest::*;
pub use materialize::*;
pub use runtime::AgentRuntimeManager;
pub use types::*;
pub use upstream_probe::*;
