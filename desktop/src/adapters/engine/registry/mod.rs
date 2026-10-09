//! Engine 命令注册表模块化收口入口
//!
//! 负责定义 Engine 命令元数据、跨领域请求派发调度以及契约 Schema 导出。

#[macro_use]
pub(crate) mod types;
pub(crate) mod dispatch;

pub(crate) mod agents;
pub(crate) mod catalog;
pub(crate) mod conversations;
pub(crate) mod memory;
pub(crate) mod mounting;
pub(crate) mod system;

pub(crate) use dispatch::*;
pub(crate) use types::*;

static ALL_COMMANDS: std::sync::LazyLock<Vec<CommandSpec>> = std::sync::LazyLock::new(|| {
    let mut specs = Vec::with_capacity(260);
    specs.extend_from_slice(system::COMMANDS);
    specs.extend_from_slice(memory::COMMANDS);
    specs.extend_from_slice(mounting::COMMANDS);
    specs.extend_from_slice(catalog::COMMANDS);
    specs.extend_from_slice(agents::COMMANDS);
    specs.extend_from_slice(&conversations::COMMANDS);
    specs
});

pub(crate) fn command_specs() -> &'static [CommandSpec] {
    &ALL_COMMANDS
}

pub(crate) fn find(method: &str) -> Option<&'static CommandSpec> {
    command_specs()
        .iter()
        .find(|spec| spec.method == method)
        .or_else(|| {
            command_specs()
                .iter()
                .find(|spec| spec.canonical_method == method)
        })
}

#[cfg(test)]
pub(crate) fn is_app_method(method: &str) -> bool {
    find(method).is_some_and(|spec| spec.exposure == CommandExposure::App)
}

#[cfg(test)]
#[path = "mod_tests.rs"]
mod tests;
