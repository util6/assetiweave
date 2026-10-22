use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, thiserror::Error, PartialEq, Eq)]
#[error("{message}")]
pub struct CliError {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
}

impl CliError {
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            hint: None,
        }
    }

    pub fn with_hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }

    pub fn new_with_hint(
        code: impl Into<String>,
        message: impl Into<String>,
        hint: impl Into<String>,
    ) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            hint: Some(hint.into()),
        }
    }

    pub fn from_anyhow(err: &anyhow::Error) -> Self {
        if let Some(cli_err) = err.downcast_ref::<CliError>() {
            cli_err.clone()
        } else {
            CliError::new("COMMAND_FAILED", format!("{err:#}"))
        }
    }

    pub fn card_not_found(card_id: impl std::fmt::Display) -> Self {
        Self::new_with_hint(
            "CARD_NOT_FOUND",
            format!("Conversation card not found: {card_id}"),
            "run 'aiwc conversation outline' or 'aiwc conversation search'",
        )
    }

    pub fn outline_not_found(id: impl std::fmt::Display) -> Self {
        Self::new_with_hint(
            "OUTLINE_NOT_FOUND",
            format!("Conversation outline not found: {id}"),
            "run 'aiwc conversation list'",
        )
    }

    pub fn adapter_not_found(adapter_id: impl std::fmt::Display) -> Self {
        Self::new_with_hint(
            "ADAPTER_NOT_FOUND",
            format!("Conversation adapter not found: {adapter_id}"),
            "run 'aiwc conversation adapter list'",
        )
    }

    pub fn search_query_required() -> Self {
        Self::new_with_hint(
            "INVALID_PARAMETER",
            "Conversation search query is required",
            "run 'aiwc conversation search --query <QUERY>' or 'aiwc conversation list'",
        )
    }
}

#[cfg(test)]
#[path = "error_tests.rs"]
mod tests;
