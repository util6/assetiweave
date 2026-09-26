use super::prelude::*;

pub(crate) fn parse_external_adapter_output_impl(
    method: &str,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    manifest: Option<&ConversationAdapterManifest>,
) -> InfraResult<ExternalAdapterRunResult> {
    let stdout = String::from_utf8(stdout).map_err(InfraError::external)?;
    let stderr = String::from_utf8_lossy(&stderr).to_string();
    let mut session_descriptors = Vec::new();
    let mut snapshot_complete = false;
    let mut sessions = Vec::new();
    let mut command_projections = Vec::new();
    let mut usage_events = Vec::new();
    let mut next_cursor = None;
    let mut decoder_profile = None;
    let mut diagnostics = Vec::new();
    let mut markdown_export = None;
    let mut warnings = Vec::new();
    let mut saw_complete = false;
    let mut item_count = 0usize;
    let mut legacy_cards_upgraded = 0usize;

    for (index, line) in stdout.lines().enumerate() {
        let line_bytes = line.as_bytes().len();
        validate_external_adapter_item_line_size(index + 1, line_bytes)?;
        if line.trim().is_empty() {
            continue;
        }
        let parsed: ExternalAdapterLine = serde_json::from_str(line).map_err(|error| {
            InfraError::external(format!(
                "invalid adapter NDJSON line {}: {error}",
                index + 1
            ))
        })?;
        let is_large_item = parsed.kind == "item"
            && parsed.item.as_ref().is_some_and(|item| {
                matches!(adapter_item_kind(item), "session" | "markdown_export")
            });
        validate_external_adapter_line_size(index + 1, line_bytes, is_large_item)?;
        match parsed.kind.as_str() {
            "item" => {
                item_count += 1;
                let item = parsed.item.ok_or_else(|| {
                    InfraError::external(format!("adapter item line {} missing item", index + 1))
                })?;
                match adapter_item_kind(&item) {
                    "session_descriptor" => {
                        session_descriptors.push(parse_adapter_session_descriptor_item(item)?);
                    }
                    "session" => {
                        if let Some((session, upgraded)) =
                            parse_adapter_session_item(item, manifest)?
                        {
                            legacy_cards_upgraded += upgraded;
                            sessions.push(session);
                        }
                    }
                    "markdown_export" => {
                        if markdown_export.is_some() {
                            return Err(InfraError::external(format!(
                                "adapter returned multiple markdown_export items by line {}",
                                index + 1
                            )));
                        }
                        markdown_export = Some(parse_adapter_markdown_export_item(item)?);
                    }
                    "command_projection" => {
                        command_projections.push(parse_adapter_command_projection_item(item)?);
                    }
                    "usage_event" => {
                        usage_events.push(parse_adapter_usage_event_item(item)?);
                    }
                    _ => {}
                }
            }
            "usage_event" => {
                item_count += 1;
                let event_val = parsed
                    .usage_event
                    .or(parsed.event)
                    .or(parsed.item)
                    .ok_or_else(|| {
                        InfraError::external(format!(
                            "adapter usage_event line {} missing event payload",
                            index + 1
                        ))
                    })?;
                usage_events.push(parse_adapter_usage_event_item(event_val)?);
            }
            "warning" => warnings.push(
                parsed
                    .message
                    .unwrap_or_else(|| "adapter warning".to_string()),
            ),
            "complete" => {
                saw_complete = true;
                if let Some(ref item) = parsed.item {
                    snapshot_complete = item
                        .get("snapshot_complete")
                        .and_then(Value::as_bool)
                        .unwrap_or(false);
                    next_cursor = item
                        .get("next_cursor")
                        .and_then(Value::as_str)
                        .map(ToString::to_string);
                    decoder_profile = item
                        .get("decoder_profile")
                        .and_then(Value::as_str)
                        .map(ToString::to_string);
                    if let Some(diag) = item.get("diagnostics") {
                        if let Some(arr) = diag.as_array() {
                            diagnostics.extend(
                                arr.iter()
                                    .filter_map(|v| v.as_str().map(ToString::to_string)),
                            );
                        } else if let Some(s) = diag.as_str() {
                            diagnostics.push(s.to_string());
                        } else {
                            diagnostics.push(diag.to_string());
                        }
                    }
                }
            }
            "error" => {
                return Err(InfraError::external(format!(
                    "adapter returned error on line {}: {}",
                    index + 1,
                    parsed
                        .error
                        .map(|value| value.to_string())
                        .or(parsed.message)
                        .unwrap_or_else(|| "unknown adapter error".to_string())
                )));
            }
            "progress" => {
                // Progress lines are valid in the protocol; ignored during final item aggregation.
            }
            other => {
                return Err(InfraError::external(format!(
                    "unsupported adapter output type on line {}: {other}",
                    index + 1
                )))
            }
        }
    }
    if !saw_complete {
        return Err(InfraError::external(
            "adapter output did not include a complete line".to_string(),
        ));
    }
    Ok(ExternalAdapterRunResult {
        method: method.to_string(),
        item_count,
        warning_count: warnings.len(),
        legacy_cards_upgraded,
        session_descriptors,
        snapshot_complete,
        sessions,
        command_projections,
        markdown_export,
        warnings,
        stderr,
        usage_events,
        next_cursor,
        decoder_profile,
        diagnostics,
    })
}

pub(crate) fn parse_adapter_usage_event_item(
    item: Value,
) -> InfraResult<crate::backend::store::conversations::usage_repo::RawUsageEventInput> {
    let event_value = item
        .get("usage_event")
        .or_else(|| item.get("event"))
        .cloned()
        .unwrap_or(item);
    serde_json::from_value(event_value)
        .map_err(|err| InfraError::external(format!("invalid adapter usage_event item: {err}")))
}

pub(crate) fn parse_adapter_command_projection_item(
    item: Value,
) -> InfraResult<ConversationCommandProjection> {
    let projection_value = item.get("projection").cloned().unwrap_or(item);
    let projection: ConversationCommandProjection =
        serde_json::from_value(projection_value).map_err(InfraError::external)?;
    if projection.part_id.trim().is_empty() {
        return Err(InfraError::external(
            "command projection part_id is required".to_string(),
        ));
    }
    if projection.schema_version != 1 {
        return Err(InfraError::external(
            "command projection schema_version must be 1".to_string(),
        ));
    }
    if projection.projector_version.trim().is_empty() {
        return Err(InfraError::external(
            "command projection projector_version is required".to_string(),
        ));
    }
    for (expected_order, node) in projection.nodes.iter().enumerate() {
        if node.display_order != expected_order {
            return Err(InfraError::external(format!(
                "command projection nodes must have contiguous display_order starting at 0: expected {expected_order}, got {}",
                node.display_order
            )));
        }
        if node.command.trim().is_empty() {
            return Err(InfraError::external(format!(
                "command projection node {expected_order} command is required"
            )));
        }
    }
    Ok(projection)
}

pub(crate) fn parse_adapter_session_descriptor_item(
    item: Value,
) -> InfraResult<ConversationSessionDescriptor> {
    let descriptor_value = item.get("descriptor").cloned().unwrap_or(item);
    let descriptor: ConversationSessionDescriptor =
        serde_json::from_value(descriptor_value).map_err(InfraError::external)?;
    if descriptor.external_id.trim().is_empty() {
        return Err(InfraError::external(
            "session descriptor external_id is required".to_string(),
        ));
    }
    if descriptor.version_token.trim().is_empty() {
        return Err(InfraError::external(
            "session descriptor version_token is required".to_string(),
        ));
    }
    Ok(descriptor)
}

pub(super) fn validate_external_adapter_line_size(
    line_number: usize,
    line_bytes: usize,
    is_large_item: bool,
) -> InfraResult<()> {
    validate_external_adapter_item_line_size(line_number, line_bytes)?;
    if line_bytes > DEFAULT_MAX_CONTROL_LINE_BYTES && !is_large_item {
        return Err(InfraError::external(format!(
            "adapter output line {line_number} exceeds max control line size ({line_bytes} bytes > {DEFAULT_MAX_CONTROL_LINE_BYTES} bytes)"
        )));
    }
    Ok(())
}

pub(crate) fn validate_external_adapter_item_line_size(
    line_number: usize,
    line_bytes: usize,
) -> InfraResult<()> {
    if line_bytes > DEFAULT_MAX_ITEM_LINE_BYTES {
        return Err(InfraError::external(format!(
            "adapter output line {line_number} exceeds max item line size ({line_bytes} bytes > {DEFAULT_MAX_ITEM_LINE_BYTES} bytes)"
        )));
    }
    Ok(())
}

pub(crate) fn adapter_item_kind(item: &Value) -> &str {
    item.get("kind")
        .and_then(Value::as_str)
        .unwrap_or("session")
}

pub(crate) fn parse_adapter_markdown_export_item(
    item: Value,
) -> InfraResult<ExternalMarkdownExport> {
    let export_value = item.get("export").cloned().unwrap_or(item);
    let export: ExternalMarkdownExport =
        serde_json::from_value(export_value).map_err(InfraError::external)?;
    if export.content.is_empty() {
        return Err(InfraError::external(
            "markdown_export content is required".to_string(),
        ));
    }
    if export.relative_path.trim().is_empty() {
        return Err(InfraError::external(
            "markdown_export relative_path is required".to_string(),
        ));
    }
    Ok(export)
}

pub(crate) fn parse_adapter_session_item(
    item: Value,
    manifest: Option<&ConversationAdapterManifest>,
) -> InfraResult<Option<(NormalizedConversationSession, usize)>> {
    let kind = adapter_item_kind(&item);
    if kind != "session" {
        return Ok(None);
    }
    let session_value = item.get("session").cloned().unwrap_or(item);
    let mut session: NormalizedConversationSession =
        serde_json::from_value(session_value).map_err(InfraError::external)?;
    let legacy_cards_upgraded = validate_normalized_session(&mut session, manifest)?;
    Ok(Some((session, legacy_cards_upgraded)))
}

pub(crate) fn validate_normalized_session(
    session: &mut NormalizedConversationSession,
    manifest: Option<&ConversationAdapterManifest>,
) -> InfraResult<usize> {
    if session.external_id.trim().is_empty() {
        return Err(InfraError::external(
            "normalized session external_id is required".to_string(),
        ));
    }
    let mut legacy_cards_upgraded = 0usize;
    for turn in &mut session.turns {
        if turn.external_id.trim().is_empty() {
            return Err(InfraError::external(
                "normalized turn external_id is required".to_string(),
            ));
        }
        if turn.user_text.trim().is_empty() {
            return Err(InfraError::external(
                "normalized turn user_text is required".to_string(),
            ));
        }
        for part in &mut turn.parts {
            remove_persisted_shell_projection(part)?;
            if let Some(manifest) = manifest {
                if crate::backend::domain::conversations::projection::canonicalize_normalized_content_card(
                    part,
                    &manifest.id,
                    manifest.card_contract_version,
                    &manifest.card_kinds,
                )
                .map_err(InfraError::external)?
                {
                    legacy_cards_upgraded += 1;
                }
            }
        }
    }
    Ok(legacy_cards_upgraded)
}

fn remove_persisted_shell_projection(
    part: &mut crate::backend::domain::NormalizedConversationPart,
) -> InfraResult<()> {
    let Some(raw) = part
        .metadata_json
        .as_deref()
        .filter(|raw| raw.contains("shell_execution_projection"))
    else {
        return Ok(());
    };
    let mut metadata = serde_json::from_str::<Value>(raw).map_err(InfraError::external)?;
    let Some(metadata_object) = metadata.as_object_mut() else {
        return Ok(());
    };
    if metadata_object
        .remove("shell_execution_projection")
        .is_none()
    {
        return Ok(());
    }
    part.metadata_json = if metadata_object.is_empty() {
        None
    } else {
        Some(serde_json::to_string(&metadata).map_err(InfraError::external)?)
    };
    Ok(())
}

pub(crate) fn example_session_detail() -> Value {
    json!({
        "session": {
            "id": "example-session",
            "source_id": "fixture-source",
            "adapter_id": "fixture-external",
            "external_id": "example-session",
            "title": "Example session",
            "project_path": "/tmp/fixture-project",
            "started_at": null,
            "updated_at": null,
            "source_locator": null,
            "source_fingerprint": null,
            "imported_at": "2026-01-01T00:00:00Z",
            "missing": false
        },
        "questions": [{
            "question": {
                "id": "example-question",
                "session_id": "example-session",
                "title": "Example question",
                "created_at": "2026-01-01T00:00:00Z",
                "updated_at": "2026-01-01T00:00:00Z"
            },
            "question_turns": [{
                "question_id": "example-question",
                "turn_id": "example-turn",
                "turn_order": 0,
                "assignment_origin": "imported",
                "assigned_at": "2026-01-01T00:00:00Z",
                "updated_at": "2026-01-01T00:00:00Z"
            }],
            "turns": [{
                "id": "example-turn",
                "session_id": "example-session",
                "external_id": "example-turn",
                "turn_index": 0,
                "user_text": "Example question",
                "title": null,
                "started_at": null,
                "ended_at": null,
                "fingerprint": "example",
                "missing": false,
                "imported_at": "2026-01-01T00:00:00Z"
            }],
            "parts": [{
                "id": "example-part",
                "turn_id": "example-turn",
                "part_index": 0,
                "role": "assistant",
                "kind": "text",
                "text": "Example answer",
                "language": null,
                "command": null,
                "cwd": null,
                "status": null,
                "exit_code": null,
                "metadata_json": "{\"content_card\":{\"type\":\"answer\",\"format\":\"markdown\"}}"
            }],
            "projected_content_nodes": [{
                "node_id": "example-part",
                "locator": {
                    "question_id": "example-question",
                    "turn_id": "example-turn",
                    "part_id": "example-part",
                    "node_order": 0
                },
                "question_id": "example-question",
                "turn_id": "example-turn",
                "part_id": "example-part",
                "turn_order": 0,
                "part_order": 0,
                "node_order": 0,
                "node_type": "answer",
                "semantic_role": "answer",
                "renderer": "markdown",
                "role": "assistant",
                "content": "Example answer",
                "language": null,
                "cwd": null,
                "status": null,
                "exit_code": null,
                "source_execution_id": null,
                "command_label": null,
                "translated_content": null,
                "legacy_anchor_ids": ["example-part-answer", "example-part-node-0"]
            }]
        }]
    })
}
