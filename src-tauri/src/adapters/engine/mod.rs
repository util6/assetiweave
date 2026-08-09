//! Engine 引擎协议与 Stdio 运行时适配层
//!
//! 提供无界面 CLI 模式下与 assetiweave-engine 的 JSON-RPC / Stdio 通信协议处理、命令注册表及策略校验。

pub(crate) mod policy;
pub(crate) mod protocol;
pub(crate) mod registry;
pub(crate) mod runtime;
mod transport;

pub(crate) use transport::run_stdio;
