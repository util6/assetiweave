//! Engine 命令注册表通用类型与宏定义

use super::super::protocol;
use crate::backend::application::AppError;
use schemars::{generate::SchemaSettings, JsonSchema};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{json, Value};

use std::future::Future;
use std::pin::Pin;

/// Engine 命令风险等级定义
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum CommandRisk {
    /// 只读操作（无破坏性）
    Read,
    /// 普通写入/更新操作
    Write,
    /// 高风险写入/删除/挂载操作（需确认提示）
    HighRiskWrite,
}

impl CommandRisk {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Read => "read",
            Self::Write => "write",
            Self::HighRiskWrite => "high-risk-write",
        }
    }
}

/// Engine 命令暴露层级定义
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CommandExposure {
    /// 对用户友好（易读 CLI 友好命令）
    Friendly,
    /// 应用内部调用接口
    App,
    /// 系统底层调优与诊断接口
    System,
}

impl CommandExposure {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Friendly => "friendly",
            Self::App => "app",
            Self::System => "system",
        }
    }
}

/// 单个命令参数元数据规范
#[derive(Clone, Copy, Debug)]
pub(crate) struct ParamSpec {
    pub(crate) name: &'static str,
    pub(crate) description: &'static str,
    pub(crate) aliases: &'static [&'static str],
}

pub(crate) type DispatchFuture = Pin<Box<dyn Future<Output = DispatchResult> + Send>>;
pub(crate) type CommandHandler = fn(Value) -> DispatchFuture;

#[derive(Clone, Copy, Debug)]
pub(crate) struct CommandSpec {
    pub(crate) method: &'static str,
    pub(crate) canonical_method: &'static str,
    pub(crate) description: &'static str,
    pub(crate) risk: CommandRisk,
    pub(crate) exposure: CommandExposure,
    pub(crate) supports_dry_run: bool,
    pub(crate) params: &'static [ParamSpec],
    pub(crate) params_schema: fn() -> Value,
    pub(crate) validate_typed_params: fn(&Value) -> Result<(), String>,
    pub(crate) handler: CommandHandler,
    pub(crate) cli: Option<&'static str>,
    pub(crate) since: &'static str,
    pub(crate) deprecated: bool,
}

impl CommandSpec {
    pub(crate) async fn dispatch(&self, params: Value) -> DispatchResult {
        (self.handler)(params).await
    }
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct NoParams {}

#[derive(Debug, Deserialize, JsonSchema)]
#[allow(dead_code)]
pub(crate) struct SchemaGetParams {
    pub(crate) method: Option<String>,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub(crate) struct RevealPathParams {
    pub(crate) path: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AgentMarketInspectParams {
    pub(crate) agent_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AgentUninstallPreviewParams {
    pub(crate) agent_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AgentToggleParams {
    pub(crate) agent_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AgentInstalledGetParams {
    pub(crate) agent_id: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub(crate) struct AgentRuntimeCheckParams {
    pub(crate) agent_id: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct ParamViolation {
    pub(crate) param: String,
    pub(crate) code: &'static str,
    pub(crate) message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) expected: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub(crate) actual: Option<String>,
}

pub(crate) type DispatchResult = Result<Value, DispatchFailure>;

#[derive(Debug)]
pub(crate) enum DispatchFailure {
    InvalidParams(String),
    OpenService(String),
    App(AppError),
    Serialize(String),
}

#[macro_export]
macro_rules! param {
    ($name:literal, $description:literal) => {
        $crate::adapters::engine::registry::types::ParamSpec {
            name: $name,
            description: $description,
            aliases: &[],
        }
    };
    ($name:literal, $description:literal, [$($alias:literal),+]) => {
        $crate::adapters::engine::registry::types::ParamSpec {
            name: $name,
            description: $description,
            aliases: &[$($alias),+],
        }
    };
}

#[macro_export]
macro_rules! command {
    (
        $method:literal,
        $canonical:literal,
        $description:literal,
        $risk:ident,
        $exposure:ident,
        $dry_run:expr,
        $params_type:ty,
        Service => |$service:ident, $typed_params:ident| $handler:expr,
        $params:expr,
        $cli:expr
        $(, since: $since:literal, deprecated: $deprecated:expr)?
    ) => {
        command!(@build
            method: $method,
            canonical_method: $canonical,
            description: $description,
            risk: $crate::adapters::engine::registry::types::CommandRisk::$risk,
            exposure: $crate::adapters::engine::registry::types::CommandExposure::$exposure,
            supports_dry_run: $dry_run,
            params_type: $params_type,
            params: $params,
            handler: command!(@service_handler $exposure,
                |params| $crate::adapters::engine::registry::dispatch::dispatch_service(
                    params,
                    |$service: &AppService, $typed_params: $params_type| $handler,
                )
            ),
            cli: $cli,
            since: command!(@since $($since)?),
            deprecated: command!(@deprecated $($deprecated)?),
        )
    };
    (
        $method:literal,
        $canonical:literal,
        $description:literal,
        $risk:ident,
        $exposure:ident,
        $dry_run:expr,
        $params_type:ty,
        ServiceAsync => |$service:ident, $typed_params:ident| $handler:expr,
        $params:expr,
        $cli:expr
        $(, since: $since:literal, deprecated: $deprecated:expr)?
    ) => {
        command!(@build
            method: $method,
            canonical_method: $canonical,
            description: $description,
            risk: $crate::adapters::engine::registry::types::CommandRisk::$risk,
            exposure: $crate::adapters::engine::registry::types::CommandExposure::$exposure,
            supports_dry_run: $dry_run,
            params_type: $params_type,
            params: $params,
            handler: command!(@service_handler $exposure,
                |params| $crate::adapters::engine::registry::dispatch::dispatch_service_async(
                    params,
                    |$service: AppService, $typed_params: $params_type| {
                        Box::pin(async move { $handler })
                    },
                )
            ),
            cli: $cli,
            since: command!(@since $($since)?),
            deprecated: command!(@deprecated $($deprecated)?),
        )
    };
    (
        $method:literal,
        $canonical:literal,
        $description:literal,
        $risk:ident,
        $exposure:ident,
        $dry_run:expr,
        $params_type:ty,
        System => |$typed_params:ident| $handler:expr,
        $params:expr,
        $cli:expr
        $(, since: $since:literal, deprecated: $deprecated:expr)?
    ) => {
        command!(@build
            method: $method,
            canonical_method: $canonical,
            description: $description,
            risk: $crate::adapters::engine::registry::types::CommandRisk::$risk,
            exposure: $crate::adapters::engine::registry::types::CommandExposure::$exposure,
            supports_dry_run: $dry_run,
            params_type: $params_type,
            params: $params,
            handler: command!(@system_handler $exposure,
                |params| $crate::adapters::engine::registry::dispatch::dispatch_system(
                    params,
                    |$typed_params: $params_type| $handler,
                )
            ),
            cli: $cli,
            since: command!(@since $($since)?),
            deprecated: command!(@deprecated $($deprecated)?),
        )
    };
    (@build
        method: $method:expr,
        canonical_method: $canonical:expr,
        description: $description:expr,
        risk: $risk:expr,
        exposure: $exposure:expr,
        supports_dry_run: $dry_run:expr,
        params_type: $params_type:ty,
        params: $params:expr,
        handler: $handler:expr,
        cli: $cli:expr,
        since: $since:expr,
        deprecated: $deprecated:expr,
    ) => {
        $crate::adapters::engine::registry::types::CommandSpec {
            method: $method,
            canonical_method: $canonical,
            description: $description,
            risk: $risk,
            exposure: $exposure,
            supports_dry_run: $dry_run,
            params: $params,
            params_schema: $crate::adapters::engine::registry::types::params_schema_for::<$params_type>,
            validate_typed_params: $crate::adapters::engine::registry::types::validate_typed_params::<$params_type>,
            handler: $handler,
            cli: $cli,
            since: $since,
            deprecated: $deprecated,
        }
    };
    (@since) => {
        "0.1.0"
    };
    (@since $since:literal) => {
        $since
    };
    (@deprecated) => {
        false
    };
    (@deprecated $deprecated:expr) => {
        $deprecated
    };
    (@service_handler Friendly, $handler:expr) => {
        $handler
    };
    (@service_handler App, $handler:expr) => {
        $handler
    };
    (@system_handler System, $handler:expr) => {
        $handler
    };
    (@system_handler App, $handler:expr) => {
        $handler
    };
}

pub(crate) fn params_schema_for<T: JsonSchema>() -> Value {
    let generator = SchemaSettings::draft2020_12()
        .for_deserialize()
        .with(|settings| {
            settings.inline_subschemas = true;
            settings.meta_schema = None;
        })
        .into_generator();
    let mut schema = serde_json::to_value(generator.into_root_schema_for::<T>())
        .expect("serialize params schema");
    normalize_root_schema(&mut schema);
    schema
}

pub(crate) fn validate_typed_params<T: DeserializeOwned>(params: &Value) -> Result<(), String> {
    serde_json::from_value::<T>(params.clone())
        .map(|_| ())
        .map_err(|error| format!("params do not match the Rust request type: {error}"))
}

pub(crate) fn normalize_root_schema(schema: &mut Value) {
    let object = schema
        .as_object_mut()
        .expect("params schema must be an object");
    object.remove("$schema");
    object.remove("title");
    object.remove("$defs");
    object.insert("type".to_string(), json!("object"));
    object.insert("additionalProperties".to_string(), json!(false));
    object
        .entry("required".to_string())
        .or_insert_with(|| json!([]));
    object
        .entry("properties".to_string())
        .or_insert_with(|| json!({}));
    if let Some(properties) = object.get_mut("properties").and_then(Value::as_object_mut) {
        for property in properties.values_mut() {
            normalize_optional_property(property);
            if let Some(values) = property.get_mut("enum").and_then(Value::as_array_mut) {
                values.retain(|value| !value.is_null());
            }
        }
    }
}

pub(crate) fn normalize_optional_property(property: &mut Value) {
    let Some(any_of) = property.get("anyOf").and_then(Value::as_array) else {
        return;
    };
    let Some(non_null) = any_of
        .iter()
        .find(|candidate| candidate["type"] != json!("null"))
        .cloned()
    else {
        return;
    };
    let Some(base_type) = non_null["type"].as_str().map(str::to_string) else {
        return;
    };
    let mut normalized = non_null;
    let object = normalized.as_object_mut().expect("property schema");
    object.insert("type".to_string(), json!([base_type, "null"]));
    if let Some(values) = object.get_mut("enum").and_then(Value::as_array_mut) {
        values.retain(|value| !value.is_null());
    }
    *property = normalized;
}

pub(crate) fn value_kind(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(number) if number.is_i64() || number.is_u64() => "integer",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}
