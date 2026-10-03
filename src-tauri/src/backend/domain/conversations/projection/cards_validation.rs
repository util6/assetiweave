use std::collections::BTreeSet;

use serde_json::{Map, Value};

use super::cards::{resolve_content_card, ConversationCardProjectionSource};
use super::error::ProjectionError;
use super::renderer::ConversationCardRenderer;
use crate::backend::domain::{
    ConversationCardKindDefinition, ConversationContentCardDescriptor, NormalizedConversationPart,
};

pub(crate) const CONTENT_CARD_SCHEMA_VERSION: u64 = 1;
pub(crate) const MAX_CARD_KIND_LENGTH: usize = 128;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum CardParseMode {
    AdapterBoundary,
    HistoricalRead,
}

pub(crate) fn validate_normalized_content_card(
    part: &NormalizedConversationPart,
    adapter_id: &str,
    card_contract_version: Option<u32>,
    card_kinds: &[ConversationCardKindDefinition],
) -> Result<(), ProjectionError> {
    let legacy = resolve_content_card(
        ConversationCardProjectionSource {
            content_card: None,
            metadata_json: part.metadata_json.as_deref(),
            text: part.text.as_deref(),
            language: part.language.as_deref(),
            command: part.command.as_deref(),
            cwd: part.cwd.as_deref(),
            status: part.status.as_deref(),
            exit_code: part.exit_code,
        },
        CardParseMode::AdapterBoundary,
    )?;
    let Some(descriptor) = part.content_card.as_ref() else {
        return Ok(());
    };
    if card_contract_version != Some(CONTENT_CARD_SCHEMA_VERSION as u32) {
        return Err(ProjectionError::MissingContractVersion {
            adapter_id: adapter_id.to_string(),
            expected: CONTENT_CARD_SCHEMA_VERSION,
        });
    }
    if descriptor.schema_version != CONTENT_CARD_SCHEMA_VERSION as u32 {
        return Err(ProjectionError::UnsupportedSchemaVersion {
            expected: CONTENT_CARD_SCHEMA_VERSION,
            actual: Some(descriptor.schema_version as u64),
        });
    }
    validate_card_kind(&descriptor.kind)?;
    let declaration = card_kinds
        .iter()
        .find(|declaration| declaration.id == descriptor.kind)
        .ok_or_else(|| ProjectionError::UndeclaredCardKind {
            adapter_id: adapter_id.to_string(),
            kind: descriptor.kind.clone(),
        })?;
    let renderer_name = descriptor
        .renderer
        .as_deref()
        .unwrap_or(&declaration.default_renderer);
    let renderer = parse_renderer(renderer_name)?;
    if !declaration
        .allowed_renderers
        .iter()
        .any(|allowed| allowed == renderer_name)
    {
        return Err(ProjectionError::RendererNotAllowed {
            kind: descriptor.kind.clone(),
            renderer: renderer_name.to_string(),
        });
    }
    if let Some(legacy) = legacy {
        let kind_matches = legacy.kind == descriptor.kind
            || declaration.semantic_role.as_deref() == Some(legacy.kind.as_str());
        if !kind_matches || legacy.renderer != renderer {
            return Err(ProjectionError::LegacyConflict {
                descriptor_kind: descriptor.kind.clone(),
                legacy_kind: legacy.kind,
            });
        }
    }
    Ok(())
}

pub(crate) fn canonicalize_normalized_content_card(
    part: &mut NormalizedConversationPart,
    adapter_id: &str,
    card_contract_version: Option<u32>,
    card_kinds: &[ConversationCardKindDefinition],
) -> Result<bool, ProjectionError> {
    validate_normalized_content_card(part, adapter_id, card_contract_version, card_kinds)?;
    let mut legacy_upgraded = false;
    if part.content_card.is_none()
        && card_contract_version == Some(CONTENT_CARD_SCHEMA_VERSION as u32)
    {
        let legacy = resolve_content_card(
            ConversationCardProjectionSource {
                content_card: None,
                metadata_json: part.metadata_json.as_deref(),
                text: part.text.as_deref(),
                language: part.language.as_deref(),
                command: part.command.as_deref(),
                cwd: part.cwd.as_deref(),
                status: part.status.as_deref(),
                exit_code: part.exit_code,
            },
            CardParseMode::AdapterBoundary,
        )?;
        if let Some(legacy) = legacy {
            let renderer_name = renderer_name(legacy.renderer);
            let mut declarations = card_kinds.iter().filter(|declaration| {
                declaration.semantic_role.as_deref() == Some(legacy.kind.as_str())
                    && declaration
                        .allowed_renderers
                        .iter()
                        .any(|allowed| allowed == renderer_name)
            });
            if let Some(declaration) = declarations.next() {
                if declarations.next().is_some() {
                    return Err(ProjectionError::AmbiguousLegacySemanticRole {
                        adapter_id: adapter_id.to_string(),
                        semantic_role: legacy.kind,
                    });
                }
                part.content_card = Some(ConversationContentCardDescriptor {
                    schema_version: CONTENT_CARD_SCHEMA_VERSION as u32,
                    kind: declaration.id.clone(),
                    renderer: Some(renderer_name.to_string()),
                });
                legacy_upgraded = true;
            }
        }
    }
    let Some(descriptor) = part.content_card.as_mut() else {
        return Ok(legacy_upgraded);
    };
    if descriptor.renderer.is_none() {
        let declaration = card_kinds
            .iter()
            .find(|declaration| declaration.id == descriptor.kind)
            .ok_or_else(|| ProjectionError::UndeclaredCardKind {
                adapter_id: adapter_id.to_string(),
                kind: descriptor.kind.clone(),
            })?;
        descriptor.renderer = Some(declaration.default_renderer.clone());
    }
    Ok(legacy_upgraded)
}

pub(crate) fn renderer_name(renderer: ConversationCardRenderer) -> &'static str {
    renderer.as_str()
}

pub(crate) fn validate_manifest_card_kinds(
    adapter_id: &str,
    card_contract_version: Option<u32>,
    card_kinds: &[ConversationCardKindDefinition],
) -> Result<(), ProjectionError> {
    if let Some(version) = card_contract_version {
        if version != CONTENT_CARD_SCHEMA_VERSION as u32 {
            return Err(ProjectionError::ManifestValidation(format!(
                "adapter card_contract_version must be {CONTENT_CARD_SCHEMA_VERSION}"
            )));
        }
    }
    if !card_kinds.is_empty() && card_contract_version.is_none() {
        return Err(ProjectionError::ManifestValidation(
            "adapter card_kinds require card_contract_version 1".to_string(),
        ));
    }
    let namespace = format!("{}.", adapter_id.trim());
    let mut ids = BTreeSet::new();
    for declaration in card_kinds {
        validate_card_kind(&declaration.id)?;
        if !declaration.id.starts_with(&namespace) {
            return Err(ProjectionError::ManifestValidation(format!(
                "adapter card kind {:?} must use namespace {namespace:?}",
                declaration.id
            )));
        }
        if !ids.insert(declaration.id.as_str()) {
            return Err(ProjectionError::ManifestValidation(format!(
                "adapter declares duplicate conversation card kind {:?}",
                declaration.id
            )));
        }
        if let Some(semantic_role) = declaration.semantic_role.as_deref() {
            validate_card_kind(semantic_role)?;
        }
        let label = declaration.label.trim();
        if label.is_empty() || label.len() > 80 || label.chars().any(char::is_control) {
            return Err(ProjectionError::ManifestValidation(format!(
                "adapter card kind {:?} must have a printable label of at most 80 bytes",
                declaration.id
            )));
        }
        parse_renderer(&declaration.default_renderer)?;
        if declaration.allowed_renderers.is_empty() {
            return Err(ProjectionError::ManifestValidation(format!(
                "adapter card kind {:?} must allow at least one renderer",
                declaration.id
            )));
        }
        let mut renderers = BTreeSet::new();
        for renderer in &declaration.allowed_renderers {
            parse_renderer(renderer)?;
            if !renderers.insert(renderer.as_str()) {
                return Err(ProjectionError::ManifestValidation(format!(
                    "adapter card kind {:?} declares duplicate renderer {renderer:?}",
                    declaration.id
                )));
            }
        }
        if !renderers.contains(declaration.default_renderer.as_str()) {
            return Err(ProjectionError::ManifestValidation(format!(
                "adapter card kind {:?} default_renderer must be present in allowed_renderers",
                declaration.id
            )));
        }
        if declaration.icon_hint.as_deref().is_some_and(|icon| {
            icon.trim().is_empty() || icon.len() > 64 || icon.chars().any(char::is_control)
        }) {
            return Err(ProjectionError::ManifestValidation(format!(
                "adapter card kind {:?} has an invalid icon_hint",
                declaration.id
            )));
        }
    }
    Ok(())
}

pub(crate) fn validate_schema_version(card: &Map<String, Value>) -> Result<(), ProjectionError> {
    let Some(value) = card
        .get("schema_version")
        .or_else(|| card.get("schemaVersion"))
    else {
        return Ok(());
    };
    if value.as_u64() == Some(CONTENT_CARD_SCHEMA_VERSION) {
        return Ok(());
    }
    Err(ProjectionError::UnsupportedSchemaVersion {
        expected: CONTENT_CARD_SCHEMA_VERSION,
        actual: value.as_u64(),
    })
}

pub(crate) fn validate_card_kind(kind: &str) -> Result<(), ProjectionError> {
    let mut bytes = kind.bytes();
    let Some(first) = bytes.next() else {
        return Err(ProjectionError::MissingCardKind);
    };
    let valid_first = first.is_ascii_lowercase() || first.is_ascii_digit();
    let valid_rest = bytes.all(|byte| {
        byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'_' | b'-')
    });
    if !valid_first || !valid_rest || kind.len() > MAX_CARD_KIND_LENGTH {
        return Err(ProjectionError::InvalidCardKind {
            kind: kind.to_string(),
            reason: format!(
                "use 1-{MAX_CARD_KIND_LENGTH} lowercase ASCII letters, digits, dots, underscores, or hyphens"
            ),
        });
    }
    Ok(())
}

pub(crate) fn is_valid_card_kind(kind: &str) -> bool {
    validate_card_kind(kind).is_ok()
}

pub(crate) fn resolve_renderer(
    card: &Map<String, Value>,
    kind: &str,
    mode: CardParseMode,
) -> Result<ConversationCardRenderer, ProjectionError> {
    let renderer = optional_string(card, "renderer");
    let presentation_renderer = card
        .get("presentation")
        .and_then(Value::as_object)
        .and_then(|presentation| optional_string(presentation, "renderer"));
    let legacy_format = optional_string(card, "format");

    if let (Some(renderer), Some(presentation_renderer)) = (&renderer, &presentation_renderer) {
        if renderer != presentation_renderer {
            return Err(ProjectionError::Other(format!(
                "conversation content card renderer {renderer:?} conflicts with presentation renderer {presentation_renderer:?}"
            )));
        }
    }

    if let Some(renderer) = renderer.or(presentation_renderer) {
        let parsed = parse_renderer(&renderer).or_else(|error| match mode {
            CardParseMode::AdapterBoundary => Err(error),
            CardParseMode::HistoricalRead => Ok(ConversationCardRenderer::Plain),
        })?;
        if let Some(format) = legacy_format {
            let legacy = renderer_from_legacy_format(kind, Some(&format), mode)?;
            if parsed != legacy {
                return Err(ProjectionError::Other(format!(
                    "conversation content card renderer {renderer:?} conflicts with legacy format {format:?}"
                )));
            }
        }
        return Ok(parsed);
    }

    renderer_from_legacy_format(kind, legacy_format.as_deref(), mode)
}

pub(crate) fn parse_renderer(value: &str) -> Result<ConversationCardRenderer, ProjectionError> {
    match value {
        "markdown" => Ok(ConversationCardRenderer::Markdown),
        "plain" => Ok(ConversationCardRenderer::Plain),
        "path" => Ok(ConversationCardRenderer::Path),
        "json" => Ok(ConversationCardRenderer::Json),
        "code" => Ok(ConversationCardRenderer::Code),
        "command" => Ok(ConversationCardRenderer::Command),
        "terminal_output" => Ok(ConversationCardRenderer::TerminalOutput),
        "diff" => Ok(ConversationCardRenderer::Diff),
        "compact_action" => Ok(ConversationCardRenderer::CompactAction),
        other => Err(ProjectionError::UnsupportedRenderer {
            renderer: other.to_string(),
        }),
    }
}

pub(crate) fn renderer_from_legacy_format(
    kind: &str,
    format: Option<&str>,
    mode: CardParseMode,
) -> Result<ConversationCardRenderer, ProjectionError> {
    if kind == "command" {
        return Ok(ConversationCardRenderer::Command);
    }
    if kind == "code" {
        return Ok(ConversationCardRenderer::Code);
    }
    match format {
        Some("markdown") => Ok(ConversationCardRenderer::Markdown),
        Some("json") => Ok(ConversationCardRenderer::Json),
        Some("plain") if kind == "result" => Ok(ConversationCardRenderer::TerminalOutput),
        Some("plain") => Ok(ConversationCardRenderer::Plain),
        Some(other) => match mode {
            CardParseMode::AdapterBoundary => Err(ProjectionError::UnsupportedRenderer {
                renderer: other.to_string(),
            }),
            CardParseMode::HistoricalRead => Ok(ConversationCardRenderer::Plain),
        },
        None if kind == "result" => Ok(ConversationCardRenderer::TerminalOutput),
        None if matches!(kind, "answer" | "tool") => Ok(ConversationCardRenderer::Markdown),
        None => Ok(ConversationCardRenderer::Plain),
    }
}

fn optional_string(card: &Map<String, Value>, key: &str) -> Option<String> {
    card.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}
