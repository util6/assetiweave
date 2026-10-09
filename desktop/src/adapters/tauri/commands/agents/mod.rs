//! Agents 领域 Tauri Command 模块
//!
//! 包含 Agent 目录/市场、生命周期、连接测试、模型列表与 Session 视图等命令。

#[macro_use]
pub(crate) mod lifecycle;
#[macro_use]
pub(crate) mod market;
#[macro_use]
pub(crate) mod runtime;

pub(crate) use lifecycle::*;
pub(crate) use market::*;
pub(crate) use runtime::*;

#[cfg(test)]
#[path = "agents_tests.rs"]
mod tests;
