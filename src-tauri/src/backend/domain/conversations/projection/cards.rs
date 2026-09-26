use serde_json::{Map, Value};

use super::cards_validation::{
    resolve_renderer, validate_card_kind, validate_schema_version, CardParseMode,
    CONTENT_CARD_SCHEMA_VERSION,
};
use super::error::ProjectionError;
use super::renderer::ConversationCardRenderer;
use crate::backend::domain::{
    ConversationCardKindDefinition, ConversationContentCardDescriptor, ConversationPart,
    ConversationPartRole,
};

/// Internal projection candidate. Cards are not part of the conversation read-model contract;
/// they are converted to source-addressable Content Nodes before a detail DTO is returned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConversationCard {
    pub node_id: String,
    pub part_id: String,
    pub adapter_id: String,
    pub kind: String,
    pub semantic_role: Option<String>,
    pub renderer: ConversationCardRenderer,
    pub role: ConversationPartRole,
    pub body: String,
    pub language: Option<String>,
    pub cwd: Option<String>,
    pub status: Option<String>,
    pub exit_code: Option<i32>,
    pub source_execution_id: Option<String>,
    pub command_label: Option<String>,
    pub translated_body: Option<String>,
    pub legacy_anchor_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedConversationContentCard {
    pub kind: String,
    pub semantic_role: Option<String>,
    pub renderer: ConversationCardRenderer,
    pub legacy_suffix: Option<String>,
    pub body: String,
    pub language: Option<String>,
    pub cwd: Option<String>,
    pub status: Option<String>,
    pub exit_code: Option<i32>,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ConversationCardProjectionSource<'a> {
    pub content_card: Option<&'a ConversationContentCardDescriptor>,
    pub metadata_json: Option<&'a str>,
    pub text: Option<&'a str>,
    pub language: Option<&'a str>,
    pub command: Option<&'a str>,
    pub cwd: Option<&'a str>,
    pub status: Option<&'a str>,
    pub exit_code: Option<i32>,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct PersistedConversationCardProjectionSource<'a> {
    pub content_card_json: Option<&'a str>,
    pub metadata_json: Option<&'a str>,
    pub text: Option<&'a str>,
    pub language: Option<&'a str>,
    pub command: Option<&'a str>,
    pub cwd: Option<&'a str>,
    pub status: Option<&'a str>,
    pub exit_code: Option<i32>,
}

pub fn project_conversation_content_card(
    part: &ConversationPart,
    adapter_id: &str,
    card_kinds: &[ConversationCardKindDefinition],
) -> Result<Option<ConversationCard>, ProjectionError> {
    let source = ConversationCardProjectionSource {
        content_card: part.content_card.as_ref(),
        metadata_json: part.metadata_json.as_deref(),
        text: part.text.as_deref(),
        language: part.language.as_deref(),
        command: part.command.as_deref(),
        cwd: part.cwd.as_deref(),
        status: part.status.as_deref(),
        exit_code: part.exit_code,
    };
    let Some(card) = project_resolved_content_card(source, card_kinds)? else {
        return Ok(None);
    };
    let mut legacy_anchor_ids = vec![format!("{}-{}", part.id, card.kind)];
    if let Some(legacy_kind) = legacy_kind_from_metadata(part.metadata_json.as_deref()) {
        let kind_anchor = format!("{}-{legacy_kind}", part.id);
        if !legacy_anchor_ids.contains(&kind_anchor) {
            legacy_anchor_ids.push(kind_anchor);
        }
    }
    if let Some(suffix) = card.legacy_suffix.as_deref() {
        let suffix_anchor = format!("{}-{suffix}", part.id);
        if !legacy_anchor_ids.contains(&suffix_anchor) {
            legacy_anchor_ids.push(suffix_anchor);
        }
    }
    Ok(Some(ConversationCard {
        node_id: part.id.clone(),
        part_id: part.id.clone(),
        adapter_id: adapter_id.to_string(),
        kind: card.kind,
        semantic_role: card.semantic_role,
        renderer: card.renderer,
        role: part.role,
        body: card.body,
        language: card.language,
        cwd: card.cwd,
        status: card.status,
        exit_code: card.exit_code,
        source_execution_id: part.source_execution_id.clone(),
        command_label: part.command_label.clone(),
        translated_body: part.translated_text.clone(),
        legacy_anchor_ids,
    }))
}

/// Projects one persisted source Part into the compatibility Card shape.
///
/// Historical `shell_execution_projection` metadata is deliberately ignored.
/// Command splitting is a read-time concern owned by the external Adapter
/// projector and must not alter the canonical Part/Card identity here.
pub fn project_conversation_content_cards(
    part: &ConversationPart,
    adapter_id: &str,
    card_kinds: &[ConversationCardKindDefinition],
) -> Result<Vec<ConversationCard>, ProjectionError> {
    Ok(
        project_conversation_content_card(part, adapter_id, card_kinds)?
            .into_iter()
            .collect(),
    )
}

pub fn project_persisted_content_card(
    source: PersistedConversationCardProjectionSource<'_>,
    card_kinds: &[ConversationCardKindDefinition],
) -> Result<Option<ResolvedConversationContentCard>, ProjectionError> {
    let descriptor = source
        .content_card_json
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| {
            serde_json::from_str::<ConversationContentCardDescriptor>(value)
                .map_err(ProjectionError::InvalidPersistedCardJson)
        })
        .transpose()?;
    project_resolved_content_card(
        ConversationCardProjectionSource {
            content_card: descriptor.as_ref(),
            metadata_json: source.metadata_json,
            text: source.text,
            language: source.language,
            command: source.command,
            cwd: source.cwd,
            status: source.status,
            exit_code: source.exit_code,
        },
        card_kinds,
    )
}

pub fn project_resolved_content_card(
    source: ConversationCardProjectionSource<'_>,
    card_kinds: &[ConversationCardKindDefinition],
) -> Result<Option<ResolvedConversationContentCard>, ProjectionError> {
    let Some(mut card) = resolve_historical_content_card(source)? else {
        return Ok(None);
    };
    if card.semantic_role.is_none() {
        card.semantic_role = card_kinds
            .iter()
            .find(|definition| definition.id == card.kind)
            .and_then(|definition| definition.semantic_role.clone());
    }
    Ok(Some(card))
}

pub fn resolve_historical_content_card(
    source: ConversationCardProjectionSource<'_>,
) -> Result<Option<ResolvedConversationContentCard>, ProjectionError> {
    resolve_content_card(source, CardParseMode::HistoricalRead)
}

pub(crate) fn resolve_content_card(
    source: ConversationCardProjectionSource<'_>,
    mode: CardParseMode,
) -> Result<Option<ResolvedConversationContentCard>, ProjectionError> {
    if let Some(descriptor) = source.content_card {
        if descriptor.schema_version != CONTENT_CARD_SCHEMA_VERSION as u32 {
            return Err(ProjectionError::UnsupportedSchemaVersion {
                expected: CONTENT_CARD_SCHEMA_VERSION,
                actual: Some(descriptor.schema_version as u64),
            });
        }
        validate_card_kind(&descriptor.kind)?;
        let renderer = match descriptor.renderer.as_deref() {
            Some(renderer) => {
                super::cards_validation::parse_renderer(renderer).or_else(|error| match mode {
                    CardParseMode::AdapterBoundary => Err(error),
                    CardParseMode::HistoricalRead => Ok(ConversationCardRenderer::Plain),
                })?
            }
            None => ConversationCardRenderer::Plain,
        };
        let status = source.status.and_then(owned);
        let exit_code = source.exit_code;
        let Some(body) = resolved_card_body(
            default_body(source, renderer),
            renderer,
            status.is_some(),
            exit_code,
        ) else {
            return Ok(None);
        };
        return Ok(Some(ResolvedConversationContentCard {
            kind: descriptor.kind.clone(),
            semantic_role: None,
            renderer,
            legacy_suffix: legacy_suffix_from_metadata(source.metadata_json),
            body,
            language: source.language.and_then(owned),
            cwd: source.cwd.and_then(owned),
            status,
            exit_code,
        }));
    }
    let Some(metadata_json) = source
        .metadata_json
        .map(str::trim)
        .filter(|value| !value.is_empty())
    else {
        return Ok(None);
    };
    let metadata = serde_json::from_str::<Value>(metadata_json).map_err(|error| {
        ProjectionError::Other(format!("invalid conversation part metadata JSON: {error}"))
    })?;
    let Some(metadata) = metadata.as_object() else {
        return Err(ProjectionError::Other(
            "conversation part metadata must be a JSON object".to_string(),
        ));
    };
    let Some(card) = metadata
        .get("content_card")
        .or_else(|| metadata.get("contentCard"))
    else {
        return Ok(None);
    };
    let card = card.as_object().ok_or_else(|| {
        ProjectionError::Other("conversation content_card must be a JSON object".to_string())
    })?;

    validate_schema_version(card)?;
    let kind = resolve_card_kind(card)?;
    validate_card_kind(&kind)?;
    let semantic_role = optional_string(card, "semantic_role")
        .or_else(|| optional_string(card, "semanticRole"))
        .or_else(|| {
            matches!(
                kind.as_str(),
                "answer" | "tool" | "command" | "code" | "result"
            )
            .then(|| kind.clone())
        });
    if let Some(semantic_role) = semantic_role.as_deref() {
        validate_card_kind(semantic_role)?;
    }
    let renderer = resolve_renderer(card, &kind, mode)?;
    let legacy_suffix = optional_string(card, "suffix");
    let status = optional_string(card, "status").or_else(|| source.status.and_then(owned));
    let exit_code = optional_i32(card, "exit_code")
        .or_else(|| optional_i32(card, "exitCode"))
        .or(source.exit_code);
    let Some(body) = resolved_card_body(
        optional_string(card, "text").or_else(|| default_body(source, renderer)),
        renderer,
        status.is_some(),
        exit_code,
    ) else {
        return Ok(None);
    };

    Ok(Some(ResolvedConversationContentCard {
        kind,
        semantic_role,
        renderer,
        legacy_suffix,
        body,
        language: optional_string(card, "language").or_else(|| source.language.and_then(owned)),
        cwd: optional_string(card, "cwd").or_else(|| source.cwd.and_then(owned)),
        status,
        exit_code,
    }))
}

fn resolved_card_body(
    value: Option<String>,
    renderer: ConversationCardRenderer,
    has_status: bool,
    exit_code: Option<i32>,
) -> Option<String> {
    if let Some(body) = value
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    {
        return Some(body);
    }
    if renderer == ConversationCardRenderer::TerminalOutput && (has_status || exit_code.is_some()) {
        return Some(String::new());
    }
    None
}

fn legacy_suffix_from_metadata(metadata_json: Option<&str>) -> Option<String> {
    let metadata = serde_json::from_str::<Value>(metadata_json?).ok()?;
    metadata
        .as_object()?
        .get("content_card")
        .or_else(|| metadata.as_object()?.get("contentCard"))?
        .as_object()
        .and_then(|card| optional_string(card, "suffix"))
}

fn legacy_kind_from_metadata(metadata_json: Option<&str>) -> Option<String> {
    let metadata = serde_json::from_str::<Value>(metadata_json?).ok()?;
    let card = metadata
        .as_object()?
        .get("content_card")
        .or_else(|| metadata.as_object()?.get("contentCard"))?
        .as_object()?;
    optional_string(card, "type")
}

fn resolve_card_kind(card: &Map<String, Value>) -> Result<String, ProjectionError> {
    let kind = optional_string(card, "kind");
    let legacy_type = optional_string(card, "type");
    if let (Some(kind), Some(legacy_type)) = (&kind, &legacy_type) {
        if kind != legacy_type {
            return Err(ProjectionError::LegacyConflict {
                descriptor_kind: kind.clone(),
                legacy_kind: legacy_type.clone(),
            });
        }
    }
    kind.or(legacy_type).ok_or(ProjectionError::MissingCardKind)
}

fn default_body(
    source: ConversationCardProjectionSource<'_>,
    renderer: ConversationCardRenderer,
) -> Option<String> {
    let values = if renderer == ConversationCardRenderer::Command {
        [source.command, source.text]
    } else {
        [source.text, source.command]
    };
    values.into_iter().flatten().find_map(owned)
}

fn optional_string(card: &Map<String, Value>, key: &str) -> Option<String> {
    card.get(key).and_then(Value::as_str).and_then(owned)
}

fn optional_i32(card: &Map<String, Value>, key: &str) -> Option<i32> {
    card.get(key)
        .and_then(Value::as_i64)
        .and_then(|value| i32::try_from(value).ok())
}

fn owned(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_string())
}

#[cfg(test)]
#[path = "cards_tests.rs"]
mod tests;
