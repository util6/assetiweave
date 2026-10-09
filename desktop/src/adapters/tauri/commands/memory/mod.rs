//! Memory 领域 Tauri Command 模块
//!
//! 包含 Recent 记忆、Project 记忆、Context Resolve、Memory Rebuild 与 Recall 搜索/问答会话等命令。

#[macro_use]
pub(crate) mod project;
#[macro_use]
pub(crate) mod recall;
#[macro_use]
pub(crate) mod recent;
#[macro_use]
pub(crate) mod tasks;

pub(crate) use project::{get_memory_project, rebuild_memory_scope, resolve_memory_context};
pub(crate) use recall::{
    cancel_memory_recall_turn, create_memory_recall_session, get_memory_recall_session,
    search_memory_recall, send_memory_recall_turn,
};
pub(crate) use recent::{
    duplicate_memory_generation_skill, get_memory_recent_snapshot,
    reset_memory_generation_skill_to_default,
};
pub(crate) use tasks::{
    cancel_memory_public_task, get_memory_public_task, list_memory_public_tasks,
    retry_memory_public_task,
};

#[cfg(test)]
#[path = "memory_tests.rs"]
mod tests;
