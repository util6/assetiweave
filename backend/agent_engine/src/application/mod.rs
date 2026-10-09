//! Agent Engine Application Layer
//!
//! 统一提供强 AI / ACP 执行引擎服务契约与调度门面。

pub use crate::infrastructure::{
    AgentAvailability, AgentConnectionResult, AgentExecutionBackend, AgentExecutionRuntime,
    AgentExecutor, AgentModelOption, AgentModelsResult, AgentProbeError, AgentRegistry,
    AgentRegistryHandle, AiExecutionCancellation, AiExecutionCleanupReport, AiExecutionError,
    AiExecutionPhase, AiExecutionRequest, AiExecutionResult,
};
