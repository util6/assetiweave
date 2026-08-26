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
) -> Result<Option<ConversationCard>, String> {
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
) -> Result<Vec<ConversationCard>, String> {
    Ok(
        project_conversation_content_card(part, adapter_id, card_kinds)?
            .into_iter()
            .collect(),
    )
}

pub(crate) fn project_persisted_content_card(
    source: PersistedConversationCardProjectionSource<'_>,
    card_kinds: &[ConversationCardKindDefinition],
) -> Result<Option<ResolvedConversationContentCard>, String> {
    let descriptor = source
        .content_card_json
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| {
            serde_json::from_str::<ConversationContentCardDescriptor>(value).map_err(|error| {
                format!("invalid persisted conversation content card JSON: {error}")
            })
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
) -> Result<Option<ResolvedConversationContentCard>, String> {
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
) -> Result<Option<ResolvedConversationContentCard>, String> {
    resolve_content_card(source, CardParseMode::HistoricalRead)
}

pub(crate) fn validate_normalized_content_card(
    part: &NormalizedConversationPart,
    adapter_id: &str,
    card_contract_version: Option<u32>,
    card_kinds: &[ConversationCardKindDefinition],
) -> Result<(), String> {
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
        return Err(format!(
            "adapter {adapter_id} must declare card_contract_version {CONTENT_CARD_SCHEMA_VERSION} before emitting content_card"
        ));
    }
    if descriptor.schema_version != CONTENT_CARD_SCHEMA_VERSION as u32 {
        return Err(format!(
            "conversation content card schema_version must be {CONTENT_CARD_SCHEMA_VERSION}"
        ));
    }
    validate_card_kind(&descriptor.kind)?;
    let declaration = card_kinds
        .iter()
        .find(|declaration| declaration.id == descriptor.kind)
        .ok_or_else(|| {
            format!(
                "adapter {adapter_id} emitted undeclared conversation card kind {:?}",
                descriptor.kind
            )
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
        return Err(format!(
            "conversation card kind {:?} does not allow renderer {renderer_name:?}",
            descriptor.kind
        ));
    }
    if let Some(legacy) = legacy {
        let kind_matches = legacy.kind == descriptor.kind
            || declaration.semantic_role.as_deref() == Some(legacy.kind.as_str());
        if !kind_matches || legacy.renderer != renderer {
            return Err(format!(
                "structured content_card for kind {:?} conflicts with legacy metadata content_card kind {:?}",
                descriptor.kind, legacy.kind
            ));
        }
    }
    Ok(())
}

pub(crate) fn canonicalize_normalized_content_card(
    part: &mut NormalizedConversationPart,
    adapter_id: &str,
    card_contract_version: Option<u32>,
    card_kinds: &[ConversationCardKindDefinition],
) -> Result<bool, String> {
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
                    return Err(format!(
                        "adapter {adapter_id} has ambiguous card kinds for legacy semantic role {:?}",
                        legacy.kind
                    ));
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
            .ok_or_else(|| {
                format!(
                    "adapter {adapter_id} emitted undeclared conversation card kind {:?}",
                    descriptor.kind
                )
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
) -> Result<(), String> {
    if let Some(version) = card_contract_version {
        if version != CONTENT_CARD_SCHEMA_VERSION as u32 {
            return Err(format!(
                "adapter card_contract_version must be {CONTENT_CARD_SCHEMA_VERSION}"
            ));
        }
    }
    if !card_kinds.is_empty() && card_contract_version.is_none() {
        return Err("adapter card_kinds require card_contract_version 1".to_string());
    }
    let namespace = format!("{}.", adapter_id.trim());
    let mut ids = BTreeSet::new();
    for declaration in card_kinds {
        validate_card_kind(&declaration.id)?;
        if !declaration.id.starts_with(&namespace) {
            return Err(format!(
                "adapter card kind {:?} must use namespace {namespace:?}",
                declaration.id
            ));
        }
        if !ids.insert(declaration.id.as_str()) {
            return Err(format!(
                "adapter declares duplicate conversation card kind {:?}",
                declaration.id
            ));
        }
        if let Some(semantic_role) = declaration.semantic_role.as_deref() {
            validate_card_kind(semantic_role)?;
        }
        let label = declaration.label.trim();
        if label.is_empty() || label.len() > 80 || label.chars().any(char::is_control) {
            return Err(format!(
                "adapter card kind {:?} must have a printable label of at most 80 bytes",
                declaration.id
            ));
        }
        parse_renderer(&declaration.default_renderer)?;
        if declaration.allowed_renderers.is_empty() {
            return Err(format!(
                "adapter card kind {:?} must allow at least one renderer",
                declaration.id
            ));
        }
        let mut renderers = BTreeSet::new();
        for renderer in &declaration.allowed_renderers {
            parse_renderer(renderer)?;
            if !renderers.insert(renderer.as_str()) {
                return Err(format!(
                    "adapter card kind {:?} declares duplicate renderer {renderer:?}",
                    declaration.id
                ));
            }
        }
        if !renderers.contains(declaration.default_renderer.as_str()) {
            return Err(format!(
                "adapter card kind {:?} default_renderer must be present in allowed_renderers",
                declaration.id
            ));
        }
        if declaration.icon_hint.as_deref().is_some_and(|icon| {
            icon.trim().is_empty() || icon.len() > 64 || icon.chars().any(char::is_control)
        }) {
            return Err(format!(
                "adapter card kind {:?} has an invalid icon_hint",
                declaration.id
            ));
        }
    }
    Ok(())
}

fn resolve_content_card(
    source: ConversationCardProjectionSource<'_>,
    mode: CardParseMode,
) -> Result<Option<ResolvedConversationContentCard>, String> {
    if let Some(descriptor) = source.content_card {
        if descriptor.schema_version != CONTENT_CARD_SCHEMA_VERSION as u32 {
            return Err(format!(
                "conversation content card schema_version must be {CONTENT_CARD_SCHEMA_VERSION}"
            ));
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
    let metadata = serde_json::from_str::<Value>(metadata_json)
        .map_err(|error| format!("invalid conversation part metadata JSON: {error}"))?;
    let Some(metadata) = metadata.as_object() else {
        return Err("conversation part metadata must be a JSON object".to_string());
    };
    let Some(card) = metadata
        .get("content_card")
        .or_else(|| metadata.get("contentCard"))
    else {
        return Ok(None);
    };
    let card = card
        .as_object()
        .ok_or_else(|| "conversation content_card must be a JSON object".to_string())?;

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

fn validate_schema_version(card: &Map<String, Value>) -> Result<(), String> {
    let Some(value) = card
        .get("schema_version")
        .or_else(|| card.get("schemaVersion"))
    else {
        return Ok(());
    };
    if value.as_u64() == Some(CONTENT_CARD_SCHEMA_VERSION) {
        return Ok(());
    }
    Err(format!(
        "conversation content card schema_version must be {CONTENT_CARD_SCHEMA_VERSION}"
    ))
}

fn resolve_card_kind(card: &Map<String, Value>) -> Result<String, String> {
    let kind = optional_string(card, "kind");
    let legacy_type = optional_string(card, "type");
    if let (Some(kind), Some(legacy_type)) = (&kind, &legacy_type) {
        if kind != legacy_type {
            return Err(format!(
                "conversation content card kind {kind:?} conflicts with legacy type {legacy_type:?}"
            ));
        }
    }
    kind.or(legacy_type)
        .ok_or_else(|| "conversation content card kind is required".to_string())
}

fn validate_card_kind(kind: &str) -> Result<(), String> {
    let mut bytes = kind.bytes();
    let Some(first) = bytes.next() else {
        return Err("conversation content card kind is required".to_string());
    };
    let valid_first = first.is_ascii_lowercase() || first.is_ascii_digit();
    let valid_rest = bytes.all(|byte| {
        byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'_' | b'-')
    });
    if !valid_first || !valid_rest || kind.len() > MAX_CARD_KIND_LENGTH {
        return Err(format!(
            "invalid conversation content card kind {kind:?}; use 1-{MAX_CARD_KIND_LENGTH} lowercase ASCII letters, digits, dots, underscores, or hyphens"
        ));
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
) -> Result<ConversationCardRenderer, String> {
    let renderer = optional_string(card, "renderer");
    let presentation_renderer = card
        .get("presentation")
        .and_then(Value::as_object)
        .and_then(|presentation| optional_string(presentation, "renderer"));
    let legacy_format = optional_string(card, "format");

    if let (Some(renderer), Some(presentation_renderer)) = (&renderer, &presentation_renderer) {
        if renderer != presentation_renderer {
            return Err(format!(
                "conversation content card renderer {renderer:?} conflicts with presentation renderer {presentation_renderer:?}"
            ));
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
                return Err(format!(
                    "conversation content card renderer {renderer:?} conflicts with legacy format {format:?}"
                ));
            }
        }
        return Ok(parsed);
    }

    renderer_from_legacy_format(kind, legacy_format.as_deref(), mode)
}

fn parse_renderer(value: &str) -> Result<ConversationCardRenderer, String> {
    match value {
        "markdown" => Ok(ConversationCardRenderer::Markdown),
        "plain" => Ok(ConversationCardRenderer::Plain),
        "path" => Ok(ConversationCardRenderer::Path),
        "json" => Ok(ConversationCardRenderer::Json),
        "code" => Ok(ConversationCardRenderer::Code),
        "command" => Ok(ConversationCardRenderer::Command),
        "terminal_output" => Ok(ConversationCardRenderer::TerminalOutput),
        "diff" => Ok(ConversationCardRenderer::Diff),
        other => Err(format!("unsupported conversation card renderer {other:?}")),
    }
}

fn renderer_from_legacy_format(
    kind: &str,
    format: Option<&str>,
    mode: CardParseMode,
) -> Result<ConversationCardRenderer, String> {
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
            CardParseMode::AdapterBoundary => {
                Err(format!("unsupported conversation card renderer {other:?}"))
            }
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
mod tests {
    use super::*;
    use crate::backend::models::{
        ConversationPart, ConversationPartKind, ConversationPartRole, NormalizedConversationPart,
    };

    #[test]
    fn conversation_card_contract_accepts_app_specific_kind_with_supported_renderer() {
        let part = part_with_metadata(
            r#"{"content_card":{"schema_version":1,"kind":"claude-code.reasoning","semantic_role":"reasoning","renderer":"markdown"}}"#,
        );

        let card = project_conversation_content_card(&part, "claude-code", &[])
            .expect("project card")
            .expect("declared card");

        assert_eq!(card.kind, "claude-code.reasoning");
        assert_eq!(card.semantic_role.as_deref(), Some("reasoning"));
        assert_eq!(card.renderer, ConversationCardRenderer::Markdown);
        assert_eq!(card.body, "Visible body");
        assert_eq!(card.node_id, "part-1");
        assert_eq!(card.adapter_id, "claude-code");
    }

    #[test]
    fn conversation_card_contract_maps_legacy_result_plain_to_terminal_output() {
        let part = part_with_metadata(
            r#"{"content_card":{"type":"result","format":"plain","suffix":"result"}}"#,
        );

        let card = project_conversation_content_card(&part, "legacy", &[])
            .expect("project card")
            .expect("declared card");

        assert_eq!(card.kind, "result");
        assert_eq!(card.renderer, ConversationCardRenderer::TerminalOutput);
        assert_eq!(card.node_id, "part-1");
        assert_eq!(card.legacy_anchor_ids, vec!["part-1-result"]);
    }

    #[test]
    fn conversation_card_contract_keeps_status_only_result_cards() {
        let mut part = part_with_metadata(
            r#"{"content_card":{"type":"result","format":"plain","suffix":"result"}}"#,
        );
        part.text = None;
        part.status = Some("completed".to_string());
        part.exit_code = Some(0);

        let card = project_conversation_content_card(&part, "legacy", &[])
            .expect("project status-only result")
            .expect("status-only result card");

        assert_eq!(card.body, "");
        assert_eq!(card.status.as_deref(), Some("completed"));
        assert_eq!(card.exit_code, Some(0));
    }

    #[test]
    fn conversation_card_contract_accepts_explicit_diff_renderer() {
        let mut part = normalized_part_with_metadata(r#"{"source_type":"file_change"}"#);
        part.kind = ConversationPartKind::FileChange;
        part.text = Some("diff --git a/a.txt b/a.txt\n@@ -1 +1 @@\n-old\n+new".to_string());
        part.content_card = Some(ConversationContentCardDescriptor {
            schema_version: 1,
            kind: "opencode.result".to_string(),
            renderer: Some("diff".to_string()),
        });
        let declarations = vec![ConversationCardKindDefinition {
            id: "opencode.result".to_string(),
            semantic_role: Some("result".to_string()),
            label: "Result".to_string(),
            default_renderer: "terminal_output".to_string(),
            allowed_renderers: vec!["terminal_output".to_string(), "diff".to_string()],
            icon_hint: None,
        }];

        validate_normalized_content_card(&part, "opencode", Some(1), &declarations)
            .expect("validate explicit diff card");
        let projected = project_resolved_content_card(
            ConversationCardProjectionSource {
                content_card: part.content_card.as_ref(),
                text: part.text.as_deref(),
                metadata_json: part.metadata_json.as_deref(),
                ..Default::default()
            },
            &declarations,
        )
        .expect("project diff card")
        .expect("diff card");
        assert_eq!(projected.renderer, ConversationCardRenderer::Diff);
    }

    #[test]
    fn conversation_card_contract_preserves_legacy_json_renderer() {
        let part = part_with_metadata(r#"{"content_card":{"type":"tool","format":"json"}}"#);

        let card = project_conversation_content_card(&part, "legacy", &[])
            .expect("project card")
            .expect("declared card");

        assert_eq!(card.renderer, ConversationCardRenderer::Json);
    }

    #[test]
    fn conversation_card_contract_accepts_a_declared_local_path_renderer() {
        let mut part = normalized_part_with_metadata(r#"{"skill_name":"session-exporter"}"#);
        part.role = ConversationPartRole::System;
        part.kind = ConversationPartKind::Metadata;
        part.text = Some("/Users/test/.codex/skills/session-exporter/SKILL.md".to_string());
        part.content_card = Some(ConversationContentCardDescriptor {
            schema_version: 1,
            kind: "codex.skill".to_string(),
            renderer: Some("path".to_string()),
        });
        let declarations = vec![ConversationCardKindDefinition {
            id: "codex.skill".to_string(),
            semantic_role: Some("skill".to_string()),
            label: "Skill".to_string(),
            default_renderer: "path".to_string(),
            allowed_renderers: vec!["path".to_string()],
            icon_hint: Some("book-open".to_string()),
        }];

        validate_normalized_content_card(&part, "codex", Some(1), &declarations)
            .expect("validate path card");
        let projected = project_resolved_content_card(
            ConversationCardProjectionSource {
                content_card: part.content_card.as_ref(),
                metadata_json: part.metadata_json.as_deref(),
                text: part.text.as_deref(),
                ..ConversationCardProjectionSource::default()
            },
            &declarations,
        )
        .expect("project path card")
        .expect("path card");

        assert_eq!(projected.renderer, ConversationCardRenderer::Path);
        assert_eq!(projected.semantic_role.as_deref(), Some("skill"));
        assert!(projected.body.ends_with("/SKILL.md"));
    }

    #[test]
    fn conversation_card_contract_upgrades_legacy_metadata_to_namespaced_descriptor() {
        let mut part = normalized_part_with_metadata(
            r#"{"content_card":{"type":"answer","format":"markdown"}}"#,
        );
        let declarations = vec![ConversationCardKindDefinition {
            id: "claude-code.answer".to_string(),
            semantic_role: Some("answer".to_string()),
            label: "Answer".to_string(),
            default_renderer: "markdown".to_string(),
            allowed_renderers: vec!["markdown".to_string()],
            icon_hint: None,
        }];

        canonicalize_normalized_content_card(&mut part, "claude-code", Some(1), &declarations)
            .expect("upgrade legacy descriptor");

        assert_eq!(
            part.content_card,
            Some(ConversationContentCardDescriptor {
                schema_version: 1,
                kind: "claude-code.answer".to_string(),
                renderer: Some("markdown".to_string()),
            })
        );
        validate_normalized_content_card(&part, "claude-code", Some(1), &declarations)
            .expect("legacy metadata and semantic role remain compatible");
    }

    #[test]
    fn conversation_card_contract_preserves_legacy_type_anchor_after_namespacing() {
        let mut part =
            part_with_metadata(r#"{"content_card":{"type":"answer","format":"markdown"}}"#);
        part.content_card = Some(ConversationContentCardDescriptor {
            schema_version: 1,
            kind: "claude-code.answer".to_string(),
            renderer: Some("markdown".to_string()),
        });
        let definitions = vec![ConversationCardKindDefinition {
            id: "claude-code.answer".to_string(),
            semantic_role: Some("answer".to_string()),
            label: "Answer".to_string(),
            default_renderer: "markdown".to_string(),
            allowed_renderers: vec!["markdown".to_string()],
            icon_hint: None,
        }];

        let card = project_conversation_content_card(&part, "claude-code", &definitions)
            .expect("project namespaced card")
            .expect("card");

        assert_eq!(
            card.legacy_anchor_ids,
            vec!["part-1-claude-code.answer", "part-1-answer"]
        );
    }

    #[test]
    fn persisted_row_projection_matches_part_projection() {
        let definitions = vec![ConversationCardKindDefinition {
            id: "claude-code.reasoning".to_string(),
            semantic_role: Some("reasoning".to_string()),
            label: "Reasoning".to_string(),
            default_renderer: "markdown".to_string(),
            allowed_renderers: vec!["markdown".to_string()],
            icon_hint: Some("brain".to_string()),
        }];
        let descriptor =
            r#"{"schema_version":1,"kind":"claude-code.reasoning","renderer":"markdown"}"#;

        let projected = project_persisted_content_card(
            PersistedConversationCardProjectionSource {
                content_card_json: Some(descriptor),
                metadata_json: Some(r#"{"source_type":"thinking"}"#),
                text: Some("Compare both paths"),
                language: None,
                command: None,
                cwd: None,
                status: None,
                exit_code: None,
            },
            &definitions,
        )
        .expect("project persisted card")
        .expect("card");

        assert_eq!(projected.kind, "claude-code.reasoning");
        assert_eq!(projected.semantic_role.as_deref(), Some("reasoning"));
        assert_eq!(projected.renderer, ConversationCardRenderer::Markdown);
        assert_eq!(projected.body, "Compare both paths");
    }

    #[test]
    fn historical_shell_projection_metadata_does_not_split_the_raw_part() {
        let mut part = part_with_metadata(
            r#"{"shell_execution_projection":{"schema_version":1,"nodes":[{"command":"rg TODO","command_label":"inspect"},{"command":"git status --short","command_label":"status"}]}}"#,
        );
        part.role = ConversationPartRole::Tool;
        part.kind = ConversationPartKind::Command;
        part.text = None;
        part.command = Some("printf '--- inspect ---'; rg TODO; git status --short".to_string());
        part.source_execution_id = Some("execution-1".to_string());
        part.content_card = Some(ConversationContentCardDescriptor {
            schema_version: 1,
            kind: "codex.command".to_string(),
            renderer: Some("command".to_string()),
        });

        let cards = project_conversation_content_cards(&part, "codex", &[])
            .expect("project raw Codex shell Part");

        assert_eq!(cards.len(), 1);
        assert_eq!(cards[0].node_id, "part-1");
        assert_eq!(cards[0].part_id, "part-1");
        assert_eq!(
            cards[0].body,
            "printf '--- inspect ---'; rg TODO; git status --short"
        );
        assert_eq!(cards[0].command_label, None);
        assert_eq!(cards[0].source_execution_id.as_deref(), Some("execution-1"));
    }

    #[test]
    fn conversation_card_contract_rejects_structured_legacy_semantic_conflict() {
        let mut part = normalized_part_with_metadata(
            r#"{"content_card":{"type":"answer","format":"markdown"}}"#,
        );
        part.content_card = Some(ConversationContentCardDescriptor {
            schema_version: 1,
            kind: "fixture.reasoning".to_string(),
            renderer: Some("markdown".to_string()),
        });
        let declarations = vec![ConversationCardKindDefinition {
            id: "fixture.reasoning".to_string(),
            semantic_role: Some("reasoning".to_string()),
            label: "Reasoning".to_string(),
            default_renderer: "markdown".to_string(),
            allowed_renderers: vec!["markdown".to_string()],
            icon_hint: None,
        }];

        let error = validate_normalized_content_card(&part, "fixture", Some(1), &declarations)
            .expect_err("different legacy semantics must conflict");

        assert!(error.contains("conflicts with legacy metadata"));
    }

    #[test]
    fn conversation_card_contract_rejects_invalid_new_kind_at_adapter_boundary() {
        let part = normalized_part_with_metadata(
            r#"{"content_card":{"schema_version":1,"kind":"Invalid Kind","presentation":{"renderer":"plain"}}}"#,
        );

        let error = validate_normalized_content_card(&part, "fixture", None, &[])
            .expect_err("invalid adapter-declared kind must fail validation");

        assert!(error.contains("content card kind"));
    }

    #[test]
    fn conversation_card_contract_rejects_unknown_new_renderer_but_reads_history_safely() {
        let metadata = r#"{"content_card":{"schema_version":1,"kind":"future-card","presentation":{"renderer":"future-ui"}}}"#;
        let normalized = normalized_part_with_metadata(metadata);
        let error = validate_normalized_content_card(&normalized, "fixture", None, &[])
            .expect_err("unsupported new renderer must fail validation");
        assert!(error.contains("unsupported conversation card renderer"));

        let historical = part_with_metadata(metadata);
        let card = project_conversation_content_card(&historical, "future", &[])
            .expect("historical projection must stay readable")
            .expect("historical card");
        assert_eq!(card.kind, "future-card");
        assert_eq!(card.renderer, ConversationCardRenderer::Plain);
    }

    #[test]
    fn conversation_card_contract_ignores_parts_without_a_card_declaration() {
        let mut part = part_with_metadata(r#"{"source_type":"assistant"}"#);
        part.metadata_json = Some(r#"{"source_type":"assistant"}"#.to_string());

        assert!(project_conversation_content_card(&part, "legacy", &[])
            .expect("project undeclared part")
            .is_none());
    }

    fn part_with_metadata(metadata: &str) -> ConversationPart {
        ConversationPart {
            id: "part-1".to_string(),
            turn_id: "turn-1".to_string(),
            part_index: 0,
            role: ConversationPartRole::Assistant,
            kind: ConversationPartKind::Text,
            text: Some("Visible body".to_string()),
            language: None,
            command: None,
            cwd: None,
            status: None,
            exit_code: None,
            command_label: None,
            source_execution_id: None,
            content_card: None,
            metadata_json: Some(metadata.to_string()),
            translated_text: None,
        }
    }

    fn normalized_part_with_metadata(metadata: &str) -> NormalizedConversationPart {
        NormalizedConversationPart {
            role: ConversationPartRole::Assistant,
            kind: ConversationPartKind::Text,
            text: Some("Visible body".to_string()),
            language: None,
            command: None,
            cwd: None,
            status: None,
            exit_code: None,
            command_label: None,
            source_execution_id: None,
            content_card: None,
            metadata_json: Some(metadata.to_string()),
        }
    }
}
