pub mod application;
pub mod domain;
pub mod infrastructure;

pub use self as backend;

// 常用顶级再导出
pub use domain::*;
pub use infrastructure::{
    AgentAvailability, AgentConnectionResult, AgentExecutionBackend, AgentExecutionRuntime,
    AgentExecutor, AgentModelOption, AgentModelsResult, AgentProbeError, AgentRegistry,
    AgentRegistryHandle, AiExecutionCancellation, AiExecutionCleanupReport, AiExecutionError,
    AiExecutionPhase, AiExecutionRequest, AiExecutionResult, ManagedAgentProcess,
    ManagedProcessError, PreparedSandbox, ProcessExit, ProcessTerminationReport, SandboxContext,
    SandboxError,
};
