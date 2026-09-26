use crate::backend::infrastructure::{
    app_settings::{
        conversation_adapter_dir, AppLocale, DEFAULT_AI_RUNTIME_CLI,
        DEFAULT_CONVERSATION_FULL_SYNC_ON_STARTUP,
    },
    InfraError, InfraResult,
};
use serde_json::{json, Value};
use std::path::{Path, PathBuf};

pub(crate) fn normalize_settings_paths(mut settings: Value) -> InfraResult<Value> {
    normalize_shared_ai_settings(&mut settings);
    for path in [
        &["dataBackup", "customDirectory"][..],
        &["conversationRuntimeOverrides", "bash"][..],
        &["conversationRuntimeOverrides", "node"][..],
        &["conversationRuntimeOverrides", "python"][..],
    ] {
        normalize_json_path_setting(&mut settings, path)?;
    }
    Ok(settings)
}

pub(crate) fn canonicalize_settings(settings: Value) -> InfraResult<Value> {
    let mut settings = normalize_settings_paths(settings)?;
    let Some(root) = settings.as_object_mut() else {
        return Ok(settings);
    };
    // These maps were migration inputs. Canonical action assignments contain
    // the complete agent/model selection and are the only execution source.
    root.remove("agentCapabilityAssignments");
    root.remove("agentModels");
    if let Some(translation) = root
        .get_mut("conversationTranslation")
        .and_then(Value::as_object_mut)
    {
        translation.remove("cli");
        translation.remove("model");
    }

    if let Some(locale_val) = root.get("locale") {
        if !locale_val.is_null() {
            match locale_val.as_str() {
                Some("zh") | Some("en") => {}
                _ => {
                    return Err(InfraError::Validation(format!(
                        "invalid locale value: {locale_val}"
                    )));
                }
            }
        }
    } else {
        root.insert("locale".to_string(), Value::Null);
    }

    if let Some(layouts_val) = root.get("columnLayouts") {
        if let Some(layouts_obj) = layouts_val.as_object() {
            for (key, array_val) in layouts_obj {
                let Some(arr) = array_val.as_array() else {
                    return Err(InfraError::Validation(format!(
                        "columnLayouts entry '{key}' must be an array"
                    )));
                };
                if arr.len() < 2 || arr.len() > 16 {
                    return Err(InfraError::Validation(format!(
                        "columnLayouts entry '{key}' must have between 2 and 16 elements"
                    )));
                }
                for item in arr {
                    let Some(num) = item.as_f64() else {
                        return Err(InfraError::Validation(format!(
                            "columnLayouts entry '{key}' elements must be positive numbers"
                        )));
                    };
                    if !num.is_finite() || num <= 0.0 {
                        return Err(InfraError::Validation(format!(
                            "columnLayouts entry '{key}' elements must be positive finite numbers"
                        )));
                    }
                }
            }
        } else {
            return Err(InfraError::Validation(
                "columnLayouts must be an object".to_string(),
            ));
        }
    } else {
        root.insert("columnLayouts".to_string(), json!({}));
    }

    Ok(settings)
}

fn normalize_shared_ai_settings(settings: &mut Value) {
    let Some(root) = settings.as_object_mut() else {
        return;
    };

    let legacy_translation = root
        .get("conversationTranslation")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let stored_runtime = root
        .get("aiRuntime")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();

    let cli = normalize_ai_runtime_cli(
        stored_runtime
            .get("cli")
            .or_else(|| legacy_translation.get("cli")),
    );
    let model = normalize_ai_runtime_model(
        stored_runtime
            .get("model")
            .or_else(|| legacy_translation.get("model")),
    );
    root.insert(
        "aiRuntime".to_string(),
        json!({ "cli": cli, "model": model }),
    );
    let mut agent_capabilities = root
        .get("agentCapabilityAssignments")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let supported_capabilities = [
        "cardTranslation",
        "memory",
        "memory.extraction",
        "memory.generation",
        "memory.project",
        "memory.global",
        "promptOptimization",
    ];
    let unknown_capabilities = agent_capabilities
        .keys()
        .filter(|key| !supported_capabilities.contains(&key.as_str()))
        .cloned()
        .collect::<Vec<_>>();
    for key in unknown_capabilities {
        agent_capabilities.remove(&key);
        tracing::warn!(
            action = "settings.agent_capability",
            capability = %key,
            "未知的 Agent capability 已禁用"
        );
    }
    for service_id in ["cardTranslation", "memory", "promptOptimization"] {
        let agent_id = normalize_agent_capability_agent_id(agent_capabilities.get(service_id), cli);
        agent_capabilities.insert(service_id.to_string(), Value::String(agent_id));
    }
    let memory_agent = agent_capabilities
        .get("memory")
        .and_then(Value::as_str)
        .unwrap_or(cli)
        .to_string();
    for service_id in [
        "memory.extraction",
        "memory.generation",
        "memory.project",
        "memory.global",
        "memory.recall",
    ] {
        let agent_id =
            normalize_agent_capability_agent_id(agent_capabilities.get(service_id), &memory_agent);
        agent_capabilities.insert(service_id.to_string(), Value::String(agent_id));
    }
    root.insert(
        "agentCapabilityAssignments".to_string(),
        Value::Object(agent_capabilities),
    );
    root.insert(
        "agentAssignments".to_string(),
        normalize_canonical_agent_assignments(root, cli, &model),
    );
    root.insert("settingsSchemaVersion".to_string(), json!(3));

    let mut translation = legacy_translation;
    translation.remove("cli");
    translation.remove("model");
    root.insert(
        "conversationTranslation".to_string(),
        Value::Object(translation),
    );

    let mut memory = root
        .get("memory")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let generation_enabled = memory
        .get("generationEnabled")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let usage_enabled = memory
        .get("usageEnabled")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let recent_window_hours = memory
        .get("recentWindowHours")
        .and_then(Value::as_u64)
        .unwrap_or(48);
    let watermark_time_1 = memory
        .get("watermarkTime1")
        .and_then(Value::as_str)
        .unwrap_or("02:00")
        .to_string();
    let watermark_time_2 = memory
        .get("watermarkTime2")
        .and_then(Value::as_str)
        .unwrap_or("14:00")
        .to_string();
    let generation_skill_asset_id = memory
        .get("generationSkillAssetId")
        .cloned()
        .unwrap_or(Value::Null);

    memory.insert("generationEnabled".to_string(), json!(generation_enabled));
    memory.insert("usageEnabled".to_string(), json!(usage_enabled));
    memory.insert("recentWindowHours".to_string(), json!(recent_window_hours));
    memory.insert("watermarkTime1".to_string(), json!(watermark_time_1));
    memory.insert("watermarkTime2".to_string(), json!(watermark_time_2));
    memory.insert(
        "generationSkillAssetId".to_string(),
        generation_skill_asset_id,
    );
    for key in ["excludedSessionIds", "excludedSourceIds"] {
        let values = memory
            .get(key)
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        memory.insert(key.to_string(), Value::Array(values));
    }
    root.insert("memory".to_string(), Value::Object(memory));
}

fn normalize_canonical_agent_assignments(
    root: &serde_json::Map<String, Value>,
    default_agent: &str,
    runtime_model: &str,
) -> Value {
    let legacy = root
        .get("agentCapabilityAssignments")
        .and_then(Value::as_object);
    let agent_models = root.get("agentModels").and_then(Value::as_object);
    let existing = root.get("agentAssignments").and_then(Value::as_object);
    let has_canonical_assignments = existing.is_some();
    let action_sources = [
        ("translation.card", "cardTranslation"),
        ("memory.extraction", "memory.extraction"),
        ("memory.generation", "memory.generation"),
        ("memory.project", "memory.project"),
        ("memory.global", "memory.global"),
        ("memory.recall", "memory.recall"),
        ("prompt.optimization", "promptOptimization"),
    ];
    let mut assignments = serde_json::Map::new();
    for (action_id, legacy_id) in action_sources {
        let existing_assignment = existing
            .and_then(|values| values.get(action_id))
            .and_then(Value::as_object);
        if has_canonical_assignments
            && existing_assignment.is_none()
            && !matches!(
                action_id,
                "memory.generation" | "memory.project" | "memory.global" | "memory.recall"
            )
        {
            continue;
        }
        let fallback_agent = if action_id == "memory.generation" {
            legacy
                .and_then(|values| values.get("memory"))
                .and_then(Value::as_str)
                .unwrap_or(default_agent)
        } else {
            default_agent
        };
        let legacy_agent = legacy
            .and_then(|values| values.get(legacy_id))
            .and_then(Value::as_str)
            .unwrap_or(fallback_agent);
        let agent_id = existing_assignment
            .and_then(|assignment| assignment.get("agentId"))
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .unwrap_or(legacy_agent);
        let model_id = existing_assignment
            .and_then(|assignment| assignment.get("modelId"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .or_else(|| {
                agent_models
                    .and_then(|models| models.get(agent_id))
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
            })
            .or_else(|| (agent_id == default_agent).then_some(runtime_model))
            .filter(|value| !value.is_empty());
        assignments.insert(
            action_id.to_string(),
            json!({ "agentId": agent_id, "modelId": model_id }),
        );
    }
    if let Some(existing) = existing {
        for key in existing.keys() {
            if !assignments.contains_key(key) {
                tracing::warn!(
                    action = "settings.agent_assignment",
                    action_id = %key,
                    "未知的 Agent action assignment 已隔离"
                );
            }
        }
    }
    Value::Object(assignments)
}

fn normalize_ai_runtime_cli(value: Option<&Value>) -> &'static str {
    if value.and_then(Value::as_str) == Some("gemini") {
        "gemini"
    } else {
        DEFAULT_AI_RUNTIME_CLI
    }
}

fn normalize_ai_runtime_model(value: Option<&Value>) -> String {
    let Some(value) = value.and_then(Value::as_str) else {
        return String::new();
    };
    let normalized = value
        .chars()
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .collect::<String>();
    let normalized = normalized.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.len() <= 120 {
        normalized
    } else {
        String::new()
    }
}

fn normalize_agent_capability_agent_id(value: Option<&Value>, fallback: &str) -> String {
    let Some(value) = value.and_then(Value::as_str) else {
        return fallback.to_string();
    };
    let normalized = value
        .chars()
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .collect::<String>();
    let normalized = normalized.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.is_empty() || normalized.len() > 128 {
        fallback.to_string()
    } else {
        normalized
    }
}

fn normalize_json_path_setting(value: &mut Value, path: &[&str]) -> InfraResult<()> {
    let Some((key, parents)) = path.split_last() else {
        return Ok(());
    };
    let mut current = value;
    for parent in parents {
        let Some(next) = current.get_mut(*parent) else {
            return Ok(());
        };
        current = next;
    }
    let Some(raw) = current
        .get(*key)
        .and_then(Value::as_str)
        .map(str::to_string)
    else {
        return Ok(());
    };
    let raw = raw.trim();
    if raw.is_empty() {
        return Ok(());
    }
    let normalized = crate::backend::infrastructure::path_utils::normalize_path_for_storage(raw)?;
    current[*key] = Value::String(normalized);
    Ok(())
}
