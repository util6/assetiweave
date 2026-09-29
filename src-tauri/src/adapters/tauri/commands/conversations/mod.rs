//! Conversations 领域 Tauri Command 模块
//!
//! 包含 Adapter 注册/包管理、会话列表/详情/骨架、全文/增量搜索、卡片/Prompt 翻译与数据维护等命令。

#[macro_use]
pub(crate) mod adapters;
#[macro_use]
pub(crate) mod maintenance;
#[macro_use]
pub(crate) mod packages;
#[macro_use]
pub(crate) mod search;
#[macro_use]
pub(crate) mod sessions;
#[macro_use]
pub(crate) mod sync;
#[macro_use]
pub(crate) mod translations;

pub(crate) use adapters::*;
pub(crate) use maintenance::*;
pub(crate) use packages::*;
pub(crate) use search::*;
pub(crate) use sessions::*;
pub(crate) use sync::*;
pub(crate) use translations::*;

#[cfg(test)]
#[path = "conversations_tests.rs"]
mod tests;
