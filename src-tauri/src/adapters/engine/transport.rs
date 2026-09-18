//! Stdio JSON-RPC 通信传输与引擎命令分发模块
//!
//! 实现从标准输入 (stdin) 读取逐行的 JSON 请求，解析方法名与参数，经由 Policy 鉴权及注册表分发执行，
//! 最终将格式化的 JSON 响应写回标准输出 (stdout) 的标准 Stdio 协议循环。

use super::{policy, protocol, registry as command_registry, runtime};
use crate::backend::runtime::{AppError, WireError};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::{self, Read, Write};

type EngineResult<T> = Result<T, EngineError>;

/// 解析后的引擎请求对象
#[derive(Debug)]
struct EngineRequest {
    /// 调用的方法名称
    method: String,
    /// 传入的 JSON 参数
    params: Value,
}

/// 线缆/网络传输中的原始 JSON-RPC 请求结构
#[derive(Debug, Deserialize)]
struct WireEngineRequest {
    /// 请求唯一 ID
    id: Option<String>,
    /// 方法名称
    method: String,
    /// 传入参数
    #[serde(default)]
    params: Value,
    /// 客户端要求的协议版本
    protocol_version: Option<u32>,
    /// 客户端要求的契约版本
    contract_version: Option<u32>,
}

impl From<WireEngineRequest> for EngineRequest {
    fn from(request: WireEngineRequest) -> Self {
        Self {
            method: request.method,
            params: if request.params.is_null() {
                json!({})
            } else {
                request.params
            },
        }
    }
}

/// 引擎标准 JSON 响应结构
#[derive(Debug, Serialize)]
struct EngineResponse {
    /// 对应的请求 ID
    id: Option<String>,
    /// 执行是否成功标志
    ok: bool,
    /// 成功时返回的数据字段
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<Value>,
    /// 协议元数据（包含执行耗时、版本号等）
    #[serde(skip_serializing_if = "Option::is_none")]
    meta: Option<Value>,
    /// 失败时返回的错误详情对象
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<EngineError>,
}

/// 引擎标准错误对象
#[derive(Debug, Serialize)]
pub(crate) struct EngineError {
    /// 错误分类名称 ("type")
    #[serde(rename = "type")]
    pub(crate) kind: String,
    /// 稳定错误代码
    pub(crate) code: String,
    /// 详细错误文本描述
    pub(crate) message: String,
    /// 针对开发者的修复提示 (Hint)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) hint: Option<String>,
    /// 诊断细节数据 JSON
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) details: Option<Value>,
    /// 是否建议调用方重试
    pub(crate) retryable: bool,
}

pub(crate) async fn run_stdio() -> Result<(), String> {
    let mut input = String::new();
    io::stdin()
        .read_to_string(&mut input)
        .map_err(|error| error.to_string())?;
    let request: WireEngineRequest = match serde_json::from_str(&input) {
        Ok(request) => request,
        Err(error) => {
            return write_response(EngineResponse {
                id: None,
                ok: false,
                data: None,
                meta: Some(response_meta()),
                error: Some(EngineError::validation(
                    "invalid_json",
                    format!("request body is not valid JSON: {error}"),
                    Some("send one JSON-RPC request object on stdin".to_string()),
                )),
            });
        }
    };

    let id = request.id.clone();
    let (hooks, mut invocation) = runtime::before(&request.method);
    let result = handle_wire_request(request).await;
    runtime::after(
        &hooks,
        &mut invocation,
        result.as_ref().err().map(|error| error.kind.as_str()),
    );
    let meta = response_meta_with_invocation(&invocation);
    let response = match result {
        Ok(data) => EngineResponse {
            id,
            ok: true,
            data: Some(data),
            meta: Some(meta),
            error: None,
        },
        Err(error) => EngineResponse {
            id,
            ok: false,
            data: None,
            meta: Some(meta),
            error: Some(error),
        },
    };

    write_response(response)
}

fn response_string(response: EngineResponse) -> String {
    serde_json::to_string_pretty(&response).unwrap_or_else(|_| {
        r#"{"ok":false,"error":{"type":"internal","code":"serialization","message":"failed to serialize response"}}"#.to_string()
    })
}

fn write_response(response: EngineResponse) -> Result<(), String> {
    let mut stdout = io::stdout();
    stdout
        .write_all(response_string(response).as_bytes())
        .map_err(|error| error.to_string())?;
    stdout.write_all(b"\n").map_err(|error| error.to_string())
}

fn response_meta() -> Value {
    protocol::response_meta()
}

fn response_meta_with_invocation(invocation: &runtime::Invocation) -> Value {
    let mut meta = response_meta();
    if let Some(object) = meta.as_object_mut() {
        object.insert("invocation".to_string(), runtime::response_meta(invocation));
    }
    meta
}

async fn handle_wire_request(request: WireEngineRequest) -> EngineResult<Value> {
    validate_wire_compatibility(&request)?;
    dispatch(request.into()).await
}

fn validate_wire_compatibility(request: &WireEngineRequest) -> EngineResult<()> {
    if request.method == "system.version" {
        return Ok(());
    }
    if request.protocol_version != Some(protocol::PROTOCOL_VERSION) {
        return Err(EngineError::engine_incompatible(
            "protocol_version_mismatch",
            "Engine protocol version is incompatible with this request",
            json!({
                "expected": protocol::PROTOCOL_VERSION,
                "received": request.protocol_version
            }),
        ));
    }
    if request.contract_version != Some(protocol::CONTRACT_VERSION) {
        return Err(EngineError::engine_incompatible(
            "contract_version_mismatch",
            "Engine command contract version is incompatible with this request",
            json!({
                "expected": protocol::CONTRACT_VERSION,
                "received": request.contract_version
            }),
        ));
    }
    Ok(())
}

async fn dispatch(mut request: EngineRequest) -> EngineResult<Value> {
    let method = request.method.clone();
    let spec =
        command_registry::find(&method).ok_or_else(|| EngineError::unknown_method(&method))?;
    policy::authorize(spec).map_err(EngineError::from_policy)?;
    if command_registry::requires_confirmation(spec, &request.params) {
        return Err(EngineError::confirmation_required(
            &method,
            spec.risk.as_str(),
        ));
    }
    request.params = command_registry::validate_params(spec, &request.params)
        .map_err(|violations| EngineError::invalid_params(&method, violations))?;
    spec.dispatch(request.params)
        .await
        .map_err(EngineError::from_dispatch)
}

impl EngineError {
    fn from_dispatch(failure: command_registry::DispatchFailure) -> Self {
        match failure {
            command_registry::DispatchFailure::InvalidParams(message) => Self::internal(message),
            command_registry::DispatchFailure::OpenService(message) => Self::internal(message),
            command_registry::DispatchFailure::App(error) => Self::from_app(error),
            command_registry::DispatchFailure::Serialize(message) => Self::internal(message),
        }
    }

    fn engine_incompatible(code: &str, message: &str, details: Value) -> Self {
        Self {
            kind: "engine_incompatible".to_string(),
            code: code.to_string(),
            message: message.to_string(),
            hint: Some("install the CLI and Engine from the same AssetIWeave release".to_string()),
            details: Some(details),
            retryable: false,
        }
    }

    fn unknown_method(method: &str) -> Self {
        Self {
            kind: "unknown_method".to_string(),
            code: "unknown_method".to_string(),
            message: format!("unknown engine method: {method}"),
            hint: Some("run `assetiweave-cli schema` to list supported methods".to_string()),
            details: Some(json!({ "method": method })),
            retryable: false,
        }
    }

    fn confirmation_required(method: &str, risk: &str) -> Self {
        Self {
            kind: "confirmation_required".to_string(),
            code: "confirmation_required".to_string(),
            message: format!("{method} requires explicit confirmation"),
            hint: Some("rerun with yes=true after reviewing the operation".to_string()),
            details: Some(json!({
                "method": method,
                "risk": risk
            })),
            retryable: false,
        }
    }

    fn invalid_params(method: &str, violations: Vec<command_registry::ParamViolation>) -> Self {
        Self {
            kind: "validation".to_string(),
            code: "invalid_params".to_string(),
            message: format!("invalid method params for {method}"),
            hint: Some(
                "run `assetiweave-cli schema get <method>` to inspect required params".to_string(),
            ),
            details: Some(json!({
                "method": method,
                "violations": violations
            })),
            retryable: false,
        }
    }

    fn from_policy(failure: policy::PolicyFailure) -> Self {
        Self {
            kind: failure.kind.to_string(),
            code: failure.kind.to_string(),
            message: failure.message,
            hint: Some(
                "review ASSETIWEAVE_POLICY_PATH or run a diagnostic command for details"
                    .to_string(),
            ),
            details: Some(failure.details),
            retryable: false,
        }
    }

    fn validation(code: &str, message: String, hint: Option<String>) -> Self {
        Self {
            kind: "validation".to_string(),
            code: code.to_string(),
            message,
            hint,
            details: None,
            retryable: false,
        }
    }

    fn internal(message: String) -> Self {
        Self {
            kind: "internal".to_string(),
            code: "internal".to_string(),
            message,
            hint: None,
            details: None,
            retryable: false,
        }
    }

    pub(crate) fn from_app(error: AppError) -> Self {
        let view: WireError = error.into();
        let kind = match view.code.as_str() {
            "validation_error" => "validation",
            "not_found" => "not_found",
            "conflict" => "conflict",
            "cancelled" => "cancelled",
            "timeout" => "timeout",
            "storage_error" => "storage",
            "process_error" => "process",
            "extension_error"
            | "manifest_invalid"
            | "incompatible"
            | "trust_rejected"
            | "program_not_found"
            | "launch_failed"
            | "probe_failed"
            | "output_limit_exceeded"
            | "nonzero_exit"
            | "cleanup_failed" => "extension",
            "external_error" => "external",
            _ => "operation_error",
        };
        Self {
            kind: kind.to_string(),
            code: view.code,
            message: view.message,
            hint: None,
            details: view.details,
            retryable: view.retryable,
        }
    }
}

#[cfg(test)]
#[path = "transport_tests.rs"]
mod tests;
