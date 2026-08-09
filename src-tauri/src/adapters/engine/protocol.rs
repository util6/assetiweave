//! Engine 协议通信元数据与能力声明
//!
//! 定义与外部 CLI / 客户端交互时使用的 Stdio JSON 协议版本、契约版本与 Engine 平台能力清单。

use serde_json::{json, Value};

/// 当前 Engine Stdio 通信协议版本号
pub(crate) const PROTOCOL_VERSION: u32 = 1;
/// 当前 Engine 命令契约版本号
pub(crate) const CONTRACT_VERSION: u32 = 3;

/// Engine 平台支持的功能能力特性标签清单
const CAPABILITIES: &[&str] = &[
    "command-contract-v1",
    "generated-app-commands-v1",
    "high-risk-confirmation-v1",
    "invocation-hooks-v1",
    "command-policy-v1",
    "protocol-handshake-v1",
    "runtime-param-validation-v1",
    "rust-type-schema-v1",
];

/// 生成响应头中的协议元信息（包含协议版本、契约版本和引擎版本）
pub(crate) fn response_meta() -> Value {
    json!({
        "protocol_version": PROTOCOL_VERSION,
        "contract_version": CONTRACT_VERSION,
        "engine_version": env!("CARGO_PKG_VERSION")
    })
}

/// 获取完整的 Engine 版本、协议号及支持能力清单对象
pub(crate) fn version_info() -> Value {
    json!({
        "product": "AssetIWeave",
        "engine_version": env!("CARGO_PKG_VERSION"),
        "protocol_version": PROTOCOL_VERSION,
        "contract_version": CONTRACT_VERSION,
        "capabilities": CAPABILITIES
    })
}
