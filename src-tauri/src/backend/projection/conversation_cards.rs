use super::error::ProjectionError;
use crate::backend::dto::ConversationCardRenderer;
use crate::backend::models::ConversationCardKindDefinition;
use crate::backend::models::{
    ConversationContentCardDescriptor, ConversationPart, NormalizedConversationPart,
};
use serde_json::{Map, Value};
use std::collections::BTreeSet;

const CONTENT_CARD_SCHEMA_VERSION: u64 = 1;
const MAX_CARD_KIND_LENGTH: usize = 128;

/// Internal projection candidate. Cards are not part of the conversation read-model contract;
/// they are converted to source-addressable Content Nodes before a detail DTO is returned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ConversationCard {
    pub(crate) node_id: String,
    pub(crate) part_id: String,
    pub(crate) adapter_id: String,
    pub(crate) kind: String,
    pub(crate) semantic_role: Option<String>,
    pub(crate) renderer: ConversationCardRenderer,
    pub(crate) role: crate::backend::models::ConversationPartRole,
    pub(crate) body: String,
    pub(crate) language: Option<String>,
    pub(crate) cwd: Option<String>,
    pub(crate) status: Option<String>,
    pub(crate) exit_code: Option<i32>,
    pub(crate) source_execution_id: Option<String>,
    pub(crate) command_label: Option<String>,
    pub(crate) translated_body: Option<String>,
    pub(crate) legacy_anchor_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResolvedConversationContentCard {
    pub(crate) kind: String,
    pub(crate) semantic_role: Option<String>,
    pub(crate) renderer: ConversationCardRenderer,
    pub(crate) legacy_suffix: Option<String>,
    pub(crate) body: String,
    pub(crate) language: Option<String>,
    pub(crate) cwd: Option<String>,
    pub(crate) status: Option<String>,
    pub(crate) exit_code: Option<i32>,
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct ConversationCardProjectionSource<'a> {
    pub(crate) content_card: Option<&'a ConversationContentCardDescriptor>,
    pub(crate) metadata_json: Option<&'a str>,
    pub(crate) text: Option<&'a str>,
    pub(crate) language: Option<&'a str>,
    pub(crate) command: Option<&'a str>,
    pub(crate) cwd: Option<&'a str>,
    pub(crate) status: Option<&'a str>,
    pub(crate) exit_code: Option<i32>,
}

#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct PersistedConversationCardProjectionSource<'a> {
    pub(crate) content_card_json: Option<&'a str>,
    pub(crate) metadata_json: Option<&'a str>,
    pub(crate) text: Option<&'a str>,
    pub(crate) language: Option<&'a str>,
    pub(crate) command: Option<&'a str>,
    pub(crate) cwd: Option<&'a str>,
    pub(crate) status: Option<&'a str>,
    pub(crate) exit_code: Option<i32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CardParseMode {
    AdapterBoundary,
    HistoricalRead,
}

pub(crate) fn project_conversation_content_card(
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
pub(crate) fn project_conversation_content_cards(
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

pub(crate) fn project_persisted_content_card(
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

fn project_resolved_content_card(
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

pub(crate) fn resolve_historical_content_card(
    source: ConversationCardProjectionSource<'_>,
) -> Result<Option<ResolvedConversationContentCard>, ProjectionError> {
    resolve_content_card(source, CardParseMode::HistoricalRead)
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

fn renderer_name(renderer: ConversationCardRenderer) -> &'static str {
    match renderer {
        ConversationCardRenderer::Markdown => "markdown",
        ConversationCardRenderer::Plain => "plain",
        ConversationCardRenderer::Path => "path",
        ConversationCardRenderer::Json => "json",
        ConversationCardRenderer::Code => "code",
        ConversationCardRenderer::Command => "command",
        ConversationCardRenderer::TerminalOutput => "terminal_output",
        ConversationCardRenderer::Diff => "diff",
    }
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

fn resolve_content_card(
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
            Some(renderer) => parse_renderer(renderer).or_else(|error| match mode {
                CardParseMode::AdapterBoundary => Err(error),
                CardParseMode::HistoricalRead => Ok(ConversationCardRenderer::Plain),
            })?,
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

fn validate_schema_version(card: &Map<String, Value>) -> Result<(), ProjectionError> {
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

fn validate_card_kind(kind: &str) -> Result<(), ProjectionError> {
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

fn resolve_renderer(
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

fn parse_renderer(value: &str) -> Result<ConversationCardRenderer, ProjectionError> {
    match value {
        "markdown" => Ok(ConversationCardRenderer::Markdown),
        "plain" => Ok(ConversationCardRenderer::Plain),
        "path" => Ok(ConversationCardRenderer::Path),
        "json" => Ok(ConversationCardRenderer::Json),
        "code" => Ok(ConversationCardRenderer::Code),
        "command" => Ok(ConversationCardRenderer::Command),
        "terminal_output" => Ok(ConversationCardRenderer::TerminalOutput),
        "diff" => Ok(ConversationCardRenderer::Diff),
        other => Err(ProjectionError::UnsupportedRenderer {
            renderer: other.to_string(),
        }),
    }
}

fn renderer_from_legacy_format(
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
#[path = "conversation_cards_tests.rs"]
mod tests;
