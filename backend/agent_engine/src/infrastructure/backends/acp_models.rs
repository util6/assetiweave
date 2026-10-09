use serde_json::Value;

use crate::infrastructure::{error::AiExecutionError, types::AgentModelOption};

pub fn parse_session_models(
    session: &agent_client_protocol::schema::v1::NewSessionResponse,
) -> Result<(Vec<AgentModelOption>, Option<String>), AiExecutionError> {
    let value = serde_json::to_value(session).map_err(|_| AiExecutionError::Protocol {
        operation: "session_model_catalog_serialize",
    })?;
    let config_options =
        strict_array_field(&value, &["config_options", "configOptions"])?.unwrap_or_default();
    let model_option = config_options.iter().find(|option| {
        let category = string_field(option, &["category"]).unwrap_or_default();
        let id = string_field(option, &["id"]).unwrap_or_default();
        let option_type = string_field(option, &["type", "option_type"]).unwrap_or_default();
        (category == "model" || id == "model")
            && (option_type.is_empty() || option_type == "select")
    });

    if let Some(model_option) = model_option {
        let Some(raw_options) = strict_array_field(model_option, &["options"])? else {
            return Err(AiExecutionError::Protocol {
                operation: "session_model_catalog_invalid",
            });
        };
        let models = raw_options
            .into_iter()
            .map(parse_model_option)
            .collect::<Option<Vec<_>>>()
            .ok_or(AiExecutionError::Protocol {
                operation: "session_model_catalog_invalid",
            })?;
        let current_model_id = string_field(
            model_option,
            &[
                "current_value",
                "currentValue",
                "selected_value",
                "selectedValue",
            ],
        )
        .or_else(|| string_field(&value, &["current_model_id", "currentModelId"]));
        if !models.is_empty() {
            return Ok((models, current_model_id));
        } else {
            return Err(AiExecutionError::Protocol {
                operation: "session_model_catalog_empty",
            });
        }
    }

    let raw_available = strict_array_field(&value, &["available_models", "availableModels"])?;
    if let Some(raw_options) = raw_available {
        let models = raw_options
            .into_iter()
            .map(parse_model_option)
            .collect::<Option<Vec<_>>>()
            .ok_or(AiExecutionError::Protocol {
                operation: "session_model_catalog_invalid",
            })?;
        let current_model_id = string_field(&value, &["current_model_id", "currentModelId"]);
        if !models.is_empty() {
            return Ok((models, current_model_id));
        } else {
            return Err(AiExecutionError::Protocol {
                operation: "session_model_catalog_empty",
            });
        }
    }

    let has_config = value.get("config_options").is_some() || value.get("configOptions").is_some();
    if has_config {
        return Err(AiExecutionError::Protocol {
            operation: "session_model_catalog_empty",
        });
    }

    Err(AiExecutionError::Protocol {
        operation: "session_model_catalog_unsupported",
    })
}

pub fn strict_array_field<'a>(
    value: &'a Value,
    names: &[&str],
) -> Result<Option<Vec<&'a Value>>, AiExecutionError> {
    let Some(raw) = names.iter().find_map(|name| value.get(*name)) else {
        return Ok(None);
    };
    let Some(array) = raw.as_array() else {
        return Err(AiExecutionError::Protocol {
            operation: "session_model_catalog_invalid",
        });
    };
    Ok(Some(array.iter().collect()))
}

pub fn string_field(value: &Value, names: &[&str]) -> Option<String> {
    names
        .iter()
        .find_map(|name| value.get(*name).and_then(Value::as_str))
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
}

pub fn parse_model_option(value: &Value) -> Option<AgentModelOption> {
    if let Some(id) = value.as_str().map(str::trim).filter(|id| !id.is_empty()) {
        return Some(AgentModelOption {
            id: id.to_owned(),
            label: id.to_owned(),
            description: None,
        });
    }
    let id = string_field(value, &["id", "value"])?;
    Some(AgentModelOption {
        label: string_field(value, &["label", "name"]).unwrap_or_else(|| id.clone()),
        description: string_field(value, &["description"]),
        id,
    })
}

pub fn resolve_model_for_session(
    session: &agent_client_protocol::schema::v1::NewSessionResponse,
    requested_model: Option<&str>,
    agent_id: &str,
) -> Option<String> {
    let requested = requested_model.map(str::trim).filter(|m| !m.is_empty());

    match parse_session_models(session) {
        Ok((models, current_model_id)) => {
            let available_ids = models.iter().map(|m| m.id.as_str()).collect::<Vec<_>>();
            if available_ids.is_empty() {
                return requested.map(str::to_owned);
            }

            if let Some(req) = requested {
                if available_ids.contains(&req) {
                    return Some(req.to_owned());
                }
                // For Codex, invalid/foreign model ids immediately cause -32602 JSON-RPC errors.
                // Fall back to a supported model. For other agents, try the requested model.
                if agent_id.to_ascii_lowercase().contains("codex") {
                    tracing::warn!(
                        agent_id = %agent_id,
                        requested_model = %req,
                        "Requested model is not available for Codex; selecting preferred fallback"
                    );
                    return select_fallback_model(
                        agent_id,
                        &available_ids,
                        current_model_id.as_deref(),
                    );
                }
                return Some(req.to_owned());
            }

            // No model requested
            if let Some(current) = current_model_id.as_deref() {
                if available_ids.contains(&current) {
                    return Some(current.to_owned());
                }
                tracing::warn!(
                    agent_id = %agent_id,
                    invalid_current_model = %current,
                    "Agent default model is not valid in available options; selecting valid fallback"
                );
            }

            select_fallback_model(agent_id, &available_ids, current_model_id.as_deref())
        }
        Err(_) => requested.map(str::to_owned),
    }
}

fn select_fallback_model(
    agent_id: &str,
    available_ids: &[&str],
    current_model: Option<&str>,
) -> Option<String> {
    if agent_id.to_ascii_lowercase().contains("codex") {
        if available_ids.contains(&"gpt-5.6-terra") {
            return Some("gpt-5.6-terra".to_string());
        }
        if available_ids.contains(&"gpt-5.6-luna") {
            return Some("gpt-5.6-luna".to_string());
        }
    }

    if let Some(current) = current_model {
        if available_ids.contains(&current) {
            return Some(current.to_string());
        }
    }

    available_ids.first().map(|m| (*m).to_string())
}

#[cfg(test)]
#[path = "acp_models_tests.rs"]
mod tests;
