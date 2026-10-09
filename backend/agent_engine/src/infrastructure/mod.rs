pub mod backends;
pub mod error;
pub mod executor;
pub mod executor_support;
pub mod executor_traits;
pub mod host_process;
pub mod launcher;
pub mod launcher_error;
pub mod process;
pub mod protocol;
pub mod registry;
pub mod registry_snapshot;
pub mod sandbox;
pub mod session_events;
pub mod types;
pub mod validation;

pub use backends::*;
pub use error::*;
pub use executor::*;
pub use executor_support::*;
pub use executor_traits::*;
pub use host_process::*;
pub use launcher::*;
pub use launcher_error::*;
pub use process::{
    managed_process, ManagedAgentProcess, ManagedProcessError, ProcessExit,
    ProcessTerminationReport,
};
pub use protocol::*;
pub use registry::*;
pub use registry_snapshot::*;
pub use sandbox::{PreparedSandbox, SandboxContext, SandboxError};
pub use session_events::*;
pub use types::*;
pub use validation::*;

/// 别名兼容模块，供 asset_core 等消费方以原有路径平滑承接
pub mod agent_execution {
    pub use super::*;
}
