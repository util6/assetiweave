pub(crate) mod generation_skill;
pub(crate) mod global_consolidation_candidates;
pub(crate) mod global_consolidation_persist;
pub(crate) mod global_consolidation_pipeline;
pub(crate) mod global_consolidation_view;
pub(crate) mod global_memory;
pub(crate) mod global_memory_context;
pub(crate) mod global_memory_documents;
pub(crate) mod legacy_archive;
pub(crate) mod memory_agent_session;
pub(crate) mod memory_generation_tools;
pub(crate) mod memory_maintenance_coordinator;
pub(crate) mod memory_maintenance_runner;
pub(crate) mod memory_projection;
pub(crate) mod memory_projection_markdown;
pub(crate) mod memory_public;
pub(crate) mod memory_rebuild;
pub(crate) mod memory_recall_executor;
pub(crate) mod memory_recall_prompt;
pub(crate) mod memory_recall_workflow;
pub(crate) mod memory_search;
pub(crate) mod memory_search_matching;
pub(crate) mod memory_tasks;
pub(crate) mod params;
pub(crate) mod project_consolidation_candidates;
pub(crate) mod project_consolidation_persist;
pub(crate) mod project_consolidation_pipeline;
pub(crate) mod project_consolidation_view;
pub(crate) mod project_memory;
pub(crate) mod project_memory_documents;
pub(crate) mod recent;
pub(crate) mod session_coordinator;
pub(crate) mod session_memory;
pub(crate) mod types;

pub(crate) use memory_generation_tools::MemoryGenerationToolHandler;
pub use types::{
    MemoryContextReference, MemoryContextResult, MemoryProjectView, MemoryRebuildResult,
    MemoryTaskView,
};
