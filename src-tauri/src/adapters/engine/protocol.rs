use serde_json::{json, Value};

pub(crate) const PROTOCOL_VERSION: u32 = 1;
pub(crate) const CONTRACT_VERSION: u32 = 3;

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

pub(crate) fn response_meta() -> Value {
    json!({
        "protocol_version": PROTOCOL_VERSION,
        "contract_version": CONTRACT_VERSION,
        "engine_version": env!("CARGO_PKG_VERSION")
    })
}

pub(crate) fn version_info() -> Value {
    json!({
        "product": "AssetIWeave",
        "engine_version": env!("CARGO_PKG_VERSION"),
        "protocol_version": PROTOCOL_VERSION,
        "contract_version": CONTRACT_VERSION,
        "capabilities": CAPABILITIES
    })
}
