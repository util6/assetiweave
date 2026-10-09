//! Engine 命令注册表请求派发与 Schema 生成核心

use super::types::*;
use crate::adapters::engine::protocol;
use crate::backend::application::{AppError, AppService};
use serde::{de::DeserializeOwned, Serialize};
use serde_json::{json, Value};
use std::future::Future;
use std::pin::Pin;

pub(crate) fn requires_confirmation(spec: &CommandSpec, params: &Value) -> bool {
    spec.risk == CommandRisk::HighRiskWrite
        && !(spec.supports_dry_run
            && params
                .get("dry_run")
                .and_then(Value::as_bool)
                .unwrap_or(false))
        && !params.get("yes").and_then(Value::as_bool).unwrap_or(false)
}

pub(crate) fn validate_params(
    spec: &CommandSpec,
    params: &Value,
) -> Result<Value, Vec<ParamViolation>> {
    let Some(object) = params.as_object() else {
        return Err(vec![ParamViolation {
            param: "$".to_string(),
            code: "expected_object",
            message: "method params must be a JSON object".to_string(),
            expected: Some("object".to_string()),
            actual: Some(value_kind(params).to_string()),
        }]);
    };

    let schema = contract_params_schema(spec);
    let properties = schema["properties"]
        .as_object()
        .expect("command params schema properties");
    let required = schema["required"]
        .as_array()
        .expect("command params schema required");
    let mut violations = Vec::new();
    for name in object.keys() {
        if !properties.contains_key(name)
            && find_param(spec, name).is_none()
            && !(spec.risk == CommandRisk::HighRiskWrite && name == "yes")
        {
            violations.push(ParamViolation {
                param: name.clone(),
                code: "unknown_param",
                message: format!("unknown parameter: {name}"),
                expected: None,
                actual: None,
            });
        }
    }

    for (name, property_schema) in properties {
        let aliases = find_param(spec, name).map_or(&[][..], |param| param.aliases);
        let present = std::iter::once(name.as_str())
            .chain(aliases.iter().copied())
            .filter_map(|name| object.get(name).map(|value| (name, value)))
            .collect::<Vec<_>>();
        if present.is_empty() {
            if required.contains(&json!(name)) {
                violations.push(ParamViolation {
                    param: name.clone(),
                    code: "required",
                    message: format!("missing required parameter: {name}"),
                    expected: schema_type(property_schema),
                    actual: None,
                });
            }
            continue;
        }
        if present.len() > 1 {
            violations.push(ParamViolation {
                param: name.clone(),
                code: "duplicate_alias",
                message: format!(
                    "parameter {} was provided more than once using aliases: {}",
                    name,
                    present
                        .iter()
                        .map(|(name, _)| *name)
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
                expected: None,
                actual: None,
            });
            continue;
        }
        let (provided_name, value) = present[0];
        if !value_matches_schema(value, property_schema) {
            violations.push(ParamViolation {
                param: provided_name.to_string(),
                code: "invalid_type",
                message: format!("invalid value for parameter: {provided_name}"),
                expected: schema_type(property_schema),
                actual: Some(value_kind(value).to_string()),
            });
        }
    }

    if !violations.is_empty() {
        return Err(violations);
    }

    let normalized = normalize_aliases(spec, object);
    if let Err(message) = (spec.validate_typed_params)(&normalized) {
        Err(vec![ParamViolation {
            param: "$".to_string(),
            code: "invalid_value",
            message,
            expected: None,
            actual: None,
        }])
    } else {
        Ok(normalized)
    }
}

pub(crate) fn normalize_aliases(
    spec: &CommandSpec,
    object: &serde_json::Map<String, Value>,
) -> Value {
    let mut normalized = object.clone();
    for param in spec.params {
        for alias in param.aliases {
            if let Some(value) = normalized.remove(*alias) {
                normalized.insert(param.name.to_string(), value);
            }
        }
    }
    Value::Object(normalized)
}

pub(crate) fn find_param<'a>(spec: &'a CommandSpec, name: &str) -> Option<&'a ParamSpec> {
    spec.params
        .iter()
        .find(|param| param.name == name || param.aliases.contains(&name))
}

pub(crate) fn value_matches_schema(value: &Value, schema: &Value) -> bool {
    let type_matches = schema["type"].as_str().map_or_else(
        || {
            schema["type"].as_array().is_some_and(|types| {
                types
                    .iter()
                    .filter_map(Value::as_str)
                    .any(|kind| value_matches_type(value, kind))
            })
        },
        |kind| value_matches_type(value, kind),
    );
    type_matches
        && schema["enum"].as_array().is_none_or(|values| {
            value.is_null() || values.iter().any(|candidate| candidate == value)
        })
}

pub(crate) fn value_matches_type(value: &Value, kind: &str) -> bool {
    match kind {
        "null" => value.is_null(),
        "string" => value.is_string(),
        "boolean" => value.is_boolean(),
        "integer" => value
            .as_number()
            .is_some_and(|number| number.is_i64() || number.is_u64()),
        "number" => value.is_number(),
        "object" => value.is_object(),
        "array" => value.is_array(),
        _ => false,
    }
}

pub(crate) fn schema_type(schema: &Value) -> Option<String> {
    schema["type"].as_str().map(str::to_string).or_else(|| {
        schema["type"].as_array().map(|types| {
            types
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join("|")
        })
    })
}

pub(crate) fn dispatch_service_async<P, T, E>(
    params: Value,
    handler: fn(AppService, P) -> Pin<Box<dyn Future<Output = Result<T, E>> + Send>>,
) -> DispatchFuture
where
    P: DeserializeOwned + Send + 'static,
    T: Serialize + Send + 'static,
    E: Into<AppError> + Send + 'static,
{
    Box::pin(async move {
        let params = deserialize_dispatch_params(params)?;
        let service = AppService::open_for_engine()
            .await
            .map_err(|error| DispatchFailure::OpenService(error.to_string()))?;
        let result = handler(service, params).await;
        serialize_dispatch_result(result.map_err(|error| DispatchFailure::App(error.into()))?)
    })
}

pub(crate) fn dispatch_service<P, T, E>(
    params: Value,
    handler: fn(&AppService, P) -> Result<T, E>,
) -> DispatchFuture
where
    P: DeserializeOwned + Send + 'static,
    T: Serialize + Send + 'static,
    E: Into<AppError> + Send + 'static,
{
    Box::pin(async move {
        let params = deserialize_dispatch_params(params)?;
        let service = AppService::open_for_engine()
            .await
            .map_err(|error| DispatchFailure::OpenService(error.to_string()))?;
        tokio::task::spawn_blocking(move || {
            serialize_dispatch_result(
                handler(&service, params).map_err(|error| DispatchFailure::App(error.into()))?,
            )
        })
        .await
        .map_err(|join_err| DispatchFailure::OpenService(join_err.to_string()))?
    })
}

pub(crate) fn dispatch_system<P, T>(params: Value, handler: fn(P) -> T) -> DispatchFuture
where
    P: DeserializeOwned + Send + 'static,
    T: Serialize + Send + 'static,
{
    Box::pin(async move {
        let params = deserialize_dispatch_params(params)?;
        serialize_dispatch_result(handler(params))
    })
}

pub(crate) fn deserialize_dispatch_params<T: DeserializeOwned>(
    params: Value,
) -> Result<T, DispatchFailure> {
    serde_json::from_value(params).map_err(|error| {
        DispatchFailure::InvalidParams(format!(
            "registered handler params failed after contract validation: {error}"
        ))
    })
}

pub(crate) fn serialize_dispatch_result<T: Serialize>(value: T) -> DispatchResult {
    serde_json::to_value(value).map_err(|error| DispatchFailure::Serialize(error.to_string()))
}

pub(crate) fn schema_index() -> Value {
    let methods = super::command_specs()
        .iter()
        .map(|spec| spec.method)
        .collect::<Vec<_>>();
    let commands = super::command_specs()
        .iter()
        .map(command_contract)
        .collect::<Vec<_>>();
    json!({
        "protocol_version": protocol::PROTOCOL_VERSION,
        "contract_version": protocol::CONTRACT_VERSION,
        "engine_version": env!("CARGO_PKG_VERSION"),
        "methods": methods,
        "commands": commands
    })
}

pub(crate) fn schema_get(method: &str) -> Value {
    super::find(method).map_or_else(
        || {
            json!({
                "contract_version": protocol::CONTRACT_VERSION,
                "method": method,
                "known": false,
                "params_schema": params_schema_for::<NoParams>()
            })
        },
        command_contract,
    )
}

pub(crate) fn command_contract(spec: &CommandSpec) -> Value {
    json!({
        "contract_version": protocol::CONTRACT_VERSION,
        "method": spec.method,
        "canonical_method": spec.canonical_method,
        "description": spec.description,
        "risk": spec.risk,
        "confirmation_required": spec.risk == CommandRisk::HighRiskWrite,
        "exposure": spec.exposure,
        "supports_dry_run": spec.supports_dry_run,
        "params_schema": contract_params_schema(spec),
        "cli": spec.cli,
        "since": spec.since,
        "deprecated": spec.deprecated
    })
}

pub(crate) fn contract_params_schema(spec: &CommandSpec) -> Value {
    let mut schema = (spec.params_schema)();
    let properties = schema["properties"]
        .as_object_mut()
        .expect("command params schema properties");
    for param in spec.params {
        let property = properties.get_mut(param.name).unwrap_or_else(|| {
            panic!(
                "documented command parameter {}.{} is missing from its Rust request type",
                spec.method, param.name
            )
        });
        let object = property
            .as_object_mut()
            .expect("command property schema must be an object");
        object.insert("description".to_string(), json!(param.description));
        if !param.aliases.is_empty() {
            object.insert("aliases".to_string(), json!(param.aliases));
        }
    }
    for (name, property) in properties.iter_mut() {
        property
            .as_object_mut()
            .expect("command property schema must be an object")
            .entry("description".to_string())
            .or_insert_with(|| json!(name.replace('_', " ")));
    }
    if spec.risk == CommandRisk::HighRiskWrite && !properties.contains_key("yes") {
        properties.insert(
            "yes".to_string(),
            json!({
                "type": "boolean",
                "description": "Confirm the high-risk operation"
            }),
        );
    }
    schema
}
