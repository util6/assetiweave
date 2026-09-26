use serde_json::Value;

use crate::backend::{
    application::AppError,
    domain::agents::{resolve_action, ActionId, AgentId},
    infrastructure::app_settings::BackendSettings,
};

pub(crate) fn resolve_agent_for(
    action: &ActionId,
    settings: &Value,
) -> Result<(AgentId, Option<String>), AppError> {
    resolve_action(action).map_err(AppError::Validation)?;
    let backend_settings = BackendSettings::from_value(settings)?;
    let (agent_id_str, model) = backend_settings
        .resolve_agent_for_action(action.as_str())
        .ok_or_else(|| {
            AppError::Validation(format!("missing Agent assignment: {}", action.as_str()))
        })?;
    let trimmed = agent_id_str.trim();
    if trimmed.is_empty() {
        return Err(AppError::Validation(format!(
            "invalid Agent assignment: {}",
            action.as_str()
        )));
    }
    let agent_id = match AgentId::parse(trimmed) {
        Ok(id) => id,
        Err(error) => return Err(AppError::Validation(format!("{error}"))),
    };
    let model = model
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    Ok((agent_id, model))
}

#[cfg(test)]
#[path = "composition_tests.rs"]
mod tests;
