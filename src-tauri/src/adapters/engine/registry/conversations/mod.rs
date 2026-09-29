//! Engine 命令注册表：Conversations 领域聚合入口

pub(crate) mod adapters;
pub(crate) mod packages;
pub(crate) mod search;
pub(crate) mod sessions;

use super::types::CommandSpec;

pub(super) static COMMANDS: std::sync::LazyLock<Vec<CommandSpec>> =
    std::sync::LazyLock::new(|| {
        let mut specs = Vec::with_capacity(120);
        specs.extend_from_slice(packages::COMMANDS);
        specs.extend_from_slice(adapters::COMMANDS);
        specs.extend_from_slice(sessions::COMMANDS);
        specs.extend_from_slice(search::COMMANDS);
        specs
    });
