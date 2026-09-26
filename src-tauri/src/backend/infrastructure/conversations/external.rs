use super::external_manifest::run_external_adapter_with_settings;
pub(crate) use super::external_manifest::*;
pub(crate) use super::external_parser::*;
pub(crate) use super::external_process::*;
pub(crate) use super::external_reader::*;
pub(crate) use super::external_sanitize::*;
pub(crate) use super::external_scaffold::*;
use super::prelude::*;

pub(crate) async fn run_external_adapter_read_session(
    adapter: &ConversationAdapter,
    source: &ConversationSource,
    session_id: Option<&str>,
    settings: &Value,
) -> InfraResult<ExternalAdapterRunResult> {
    validate_external_adapter_for_method(adapter, source, "read_session")?;
    let manifest_path = adapter.manifest_path.as_deref().ok_or_else(|| {
        InfraError::external({
            format!(
                "external conversation adapter has no manifest: {}",
                adapter.id
            )
        })
    })?;
    let validation = validate_external_adapter_manifest(manifest_path)?;
    validate_external_adapter_manifest_for_method(adapter, &validation, "read_session")?;
    let source_location = resolve_source_location_for_adapter(source)?;
    let request = json!({
        "protocol_version": EXTERNAL_ADAPTER_PROTOCOL_VERSION,
        "request_id": format!("sync-{}-{}", source.id, Utc::now().timestamp_millis()),
        "method": "read_session",
        "source": { "location": source_location, "config": source_config_value(source)? },
        "params": { "session_id": session_id }
    });
    run_external_adapter_with_settings(
        &validation,
        "read_session",
        request,
        Duration::from_millis(DEFAULT_READ_TIMEOUT_MS),
        settings,
    )
    .await
}

pub(crate) async fn export_external_adapter_markdown_with_settings(
    adapter: &ConversationAdapter,
    source: &ConversationSource,
    detail: &crate::backend::domain::conversations::ConversationSessionDetail,
    question_ids: &[String],
    content_filter: &crate::backend::domain::conversations::ConversationExportContentFilter,
    record_kind: &str,
    default_relative_path: &str,
    settings: &Value,
) -> InfraResult<ExternalMarkdownExport> {
    validate_external_adapter_for_method(adapter, source, "export_markdown")?;
    let manifest_path = adapter.manifest_path.as_deref().ok_or_else(|| {
        InfraError::external({
            format!(
                "external conversation adapter has no manifest: {}",
                adapter.id
            )
        })
    })?;
    let validation = validate_external_adapter_manifest(manifest_path)?;
    validate_external_adapter_manifest_for_method(adapter, &validation, "export_markdown")?;
    let source_location = resolve_source_location_for_adapter(source)?;
    let request = json!({
        "protocol_version": EXTERNAL_ADAPTER_PROTOCOL_VERSION,
        "request_id": format!("export-{}-{}", detail.session.id, Utc::now().timestamp_millis()),
        "method": "export_markdown",
        "source": { "location": source_location, "config": source_config_value(source)? },
        "params": {
            "session_detail": detail,
            "question_ids": question_ids,
            "content_filter": content_filter,
            "record_kind": record_kind,
            "default_relative_path": default_relative_path
        }
    });
    Ok(run_external_adapter_with_settings(
        &validation,
        "export_markdown",
        request,
        Duration::from_millis(DEFAULT_READ_TIMEOUT_MS),
        settings,
    )
    .await?
    .markdown_export
    .ok_or_else(|| {
        InfraError::NotFound(format!(
            "external conversation adapter {} did not return markdown_export",
            adapter.id
        ))
    })?)
}

pub(crate) fn adapter_supports_usage(adapter: &ConversationAdapter) -> bool {
    adapter
        .capabilities
        .iter()
        .any(|capability| capability == "read_usage")
}

pub(crate) async fn read_external_adapter_usage_with_settings(
    adapter: &ConversationAdapter,
    source: &ConversationSource,
    cursor: Option<&str>,
    settings: &Value,
) -> InfraResult<ExternalAdapterRunResult> {
    validate_external_adapter_for_method(adapter, source, "read_usage")?;
    let manifest_path = adapter.manifest_path.as_deref().ok_or_else(|| {
        InfraError::external(format!(
            "external conversation adapter has no manifest: {}",
            adapter.id
        ))
    })?;
    let validation = validate_external_adapter_manifest(manifest_path)?;
    validate_external_adapter_manifest_for_method(adapter, &validation, "read_usage")?;
    let source_location = resolve_source_location_for_adapter(source)?;
    let request = json!({
        "protocol_version": EXTERNAL_ADAPTER_PROTOCOL_VERSION,
        "request_id": format!("usage-{}-{}", source.id, Utc::now().timestamp_millis()),
        "method": "read_usage",
        "source": { "location": source_location, "config": source_config_value(source)? },
        "params": {
            "cursor": cursor,
            "mode": if cursor.is_some() { "incremental" } else { "full" }
        }
    });
    run_external_adapter_with_settings(
        &validation,
        "read_usage",
        request,
        Duration::from_millis(DEFAULT_READ_TIMEOUT_MS),
        settings,
    )
    .await
}

pub(crate) async fn project_external_adapter_command_parts_with_settings(
    adapter: &ConversationAdapter,
    parts: &[ConversationCommandProjectionPart],
    settings: &Value,
) -> InfraResult<Vec<ConversationCommandProjection>> {
    const METHOD: &str = "project_command_parts";
    const MAX_PARTS: usize = 128;
    const MAX_TOTAL_COMMAND_BYTES: usize = 8 * 1024 * 1024;

    if parts.is_empty() {
        return Ok(Vec::new());
    }
    if parts.len() > MAX_PARTS {
        return Err(InfraError::Validation(format!(
            "command projection batch exceeds {MAX_PARTS} Parts"
        )));
    }
    let mut requested_ids = std::collections::BTreeSet::new();
    let mut total_command_bytes = 0usize;
    for part in parts {
        if part.part_id.trim().is_empty() {
            return Err(InfraError::Validation(
                "command projection part_id is required".to_string(),
            ));
        }
        if part.command.trim().is_empty() {
            return Err(InfraError::Validation(format!(
                "command projection command is required for Part {}",
                part.part_id
            )));
        }
        if !requested_ids.insert(part.part_id.as_str()) {
            return Err(InfraError::Validation(format!(
                "duplicate command projection Part: {}",
                part.part_id
            )));
        }
        total_command_bytes = total_command_bytes.saturating_add(part.command.len());
    }
    if total_command_bytes > MAX_TOTAL_COMMAND_BYTES {
        return Err(InfraError::Validation(format!(
            "command projection batch exceeds {MAX_TOTAL_COMMAND_BYTES} command bytes"
        )));
    }
    if adapter.kind != ConversationAdapterKind::External
        || !adapter.enabled
        || !matches!(
            adapter.trust_state,
            ConversationAdapterTrustState::Trusted | ConversationAdapterTrustState::BuiltIn
        )
        || !adapter.capabilities.iter().any(|value| value == METHOD)
    {
        return Err(InfraError::external(format!(
            "conversation adapter {} is not enabled and trusted for {METHOD}",
            adapter.id
        )));
    }
    let manifest_path = adapter.manifest_path.as_deref().ok_or_else(|| {
        InfraError::external(format!(
            "external conversation adapter has no manifest: {}",
            adapter.id
        ))
    })?;
    let validation = validate_external_adapter_manifest(manifest_path)?;
    validate_external_adapter_manifest_for_method(adapter, &validation, METHOD)?;
    let manifest_dir = Path::new(&validation.manifest_path)
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    let request = json!({
        "protocol_version": EXTERNAL_ADAPTER_PROTOCOL_VERSION,
        "request_id": format!("project-{}-{}", adapter.id, Utc::now().timestamp_millis()),
        "method": METHOD,
        "source": { "location": manifest_dir, "config": null },
        "params": { "parts": parts }
    });
    let result = run_external_adapter_with_settings(
        &validation,
        METHOD,
        request,
        Duration::from_millis(DEFAULT_PROJECT_TIMEOUT_MS),
        settings,
    )
    .await?;

    let mut by_part_id = std::collections::BTreeMap::new();
    for projection in result.command_projections {
        if !requested_ids.contains(projection.part_id.as_str()) {
            return Err(InfraError::external(format!(
                "adapter returned an unknown command projection Part: {}",
                projection.part_id
            )));
        }
        let projection_id = projection.part_id.clone();
        if by_part_id
            .insert(projection_id.clone(), projection)
            .is_some()
        {
            return Err(InfraError::external(format!(
                "adapter returned duplicate command projections for Part: {projection_id}"
            )));
        }
    }
    parts
        .iter()
        .map(|part| {
            by_part_id.remove(&part.part_id).ok_or_else(|| {
                InfraError::external(format!(
                    "adapter did not return a command projection for Part: {}",
                    part.part_id
                ))
            })
        })
        .collect()
}

pub(crate) fn resolve_source_location_for_adapter(
    source: &ConversationSource,
) -> InfraResult<String> {
    if source.location.contains("://") {
        return Ok(source.location.clone());
    }
    Ok(
        crate::backend::infrastructure::path_utils::expand_path(&source.location)?
            .to_string_lossy()
            .to_string(),
    )
}

pub(crate) fn validate_external_adapter(
    params: ExternalAdapterValidateParams,
) -> InfraResult<ExternalAdapterValidationResult> {
    validate_external_adapter_manifest(&params.manifest_path)
}

pub(crate) async fn register_external_adapter_with_settings(
    params: ExternalAdapterRegisterParams,
    settings: &Value,
) -> InfraResult<Value> {
    if !params.dry_run && !params.yes {
        return Err(InfraError::external(
            "conversation.adapter.register requires --yes".to_string(),
        ));
    }
    let validation = validate_external_adapter_manifest(&params.manifest_path)?;
    let probe = if params.dry_run {
        None
    } else {
        Some(probe_external_adapter_before_trust(&validation, settings).await?)
    };
    let now = Utc::now().to_rfc3339();
    let adapter = ConversationAdapter {
        id: validation.manifest.id.clone(),
        name: validation.manifest.name.clone(),
        kind: ConversationAdapterKind::External,
        version: validation.manifest.version.clone(),
        enabled: true,
        manifest_path: Some(validation.manifest_path.clone()),
        executable_path: Some(validation.executable_path.clone()),
        content_hash: Some(validation.content_hash.clone()),
        trusted_hash: Some(validation.content_hash.clone()),
        trust_state: ConversationAdapterTrustState::Trusted,
        protocol_version: Some(validation.manifest.protocol_version),
        capabilities: validation.manifest.capabilities.clone(),
        input_kinds: validation.manifest.input_kinds.clone(),
        card_contract_version: validation.manifest.card_contract_version,
        card_kinds: validation.manifest.card_kinds.clone(),
        created_at: now.clone(),
        updated_at: now,
    };
    Ok(json!({
        "dry_run": params.dry_run,
        "adapter": adapter,
        "probe": probe,
        "validation": validation
    }))
}

#[cfg(test)]
pub(crate) async fn register_external_adapter(
    params: ExternalAdapterRegisterParams,
) -> InfraResult<Value> {
    register_external_adapter_with_settings(params, &serde_json::json!({})).await
}

async fn probe_external_adapter_before_trust(
    validation: &ExternalAdapterValidationResult,
    settings: &Value,
) -> InfraResult<ExternalAdapterRunResult> {
    if !validation
        .manifest
        .capabilities
        .iter()
        .any(|capability| capability == "probe")
    {
        return Err(InfraError::external(format!(
            "adapter {} must declare probe before it can be trusted",
            validation.manifest.id
        )));
    }
    let manifest_dir = Path::new(&validation.manifest_path)
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    let request = json!({
        "protocol_version": EXTERNAL_ADAPTER_PROTOCOL_VERSION,
        "request_id": format!("trust-probe-{}", Utc::now().timestamp_millis()),
        "method": "probe",
        "source": { "location": manifest_dir, "config": null },
        "params": {}
    });
    Ok(run_external_adapter_with_settings(
        validation,
        "probe",
        request,
        Duration::from_millis(DEFAULT_PROBE_TIMEOUT_MS),
        settings,
    )
    .await
    .map_err(|error| {
        InfraError::External(format!(
            "adapter {} probe failed; refusing to trust: {error}",
            validation.manifest.id
        ))
    })?)
}

pub(crate) async fn try_run_external_adapter_with_settings(
    params: ExternalAdapterTryRunParams,
    settings: &Value,
) -> InfraResult<ExternalAdapterRunResult> {
    if !params.yes {
        return Err(InfraError::external(
            "conversation.adapter.try-run requires --yes".to_string(),
        ));
    }
    let validation = validate_external_adapter_manifest(&params.manifest_path)?;
    let method = params.method.trim();
    if !validation
        .manifest
        .capabilities
        .iter()
        .any(|capability| capability == method)
    {
        return Err(InfraError::external(format!(
            "adapter does not declare capability: {method}"
        )));
    }
    let timeout_ms = match method {
        "probe" => DEFAULT_PROBE_TIMEOUT_MS,
        "list_sessions" => DEFAULT_LIST_TIMEOUT_MS,
        "read_session" => DEFAULT_READ_TIMEOUT_MS,
        "export_markdown" => DEFAULT_READ_TIMEOUT_MS,
        other => {
            return Err(InfraError::external(format!(
                "unsupported adapter method: {other}"
            )))
        }
    };
    let location = params.location.unwrap_or_else(|| ".".to_string());
    let request_params = if method == "export_markdown" {
        json!({
            "session_detail": example_session_detail(),
            "question_ids": params.session_id.as_ref().map(|id| vec![id.clone()]).unwrap_or_default(),
            "content_filter": {
                "answer": true,
                "tool": true,
                "command": true,
                "code": true,
                "result": true
            },
            "record_kind": "session",
            "default_relative_path": "fixture-external/fixture-project/example-session.md"
        })
    } else {
        json!({ "session_id": params.session_id })
    };
    let request = json!({
        "protocol_version": EXTERNAL_ADAPTER_PROTOCOL_VERSION,
        "request_id": format!("try-run-{}", Utc::now().timestamp_millis()),
        "method": method,
        "source": { "location": location, "config": null },
        "params": request_params
    });
    run_external_adapter_with_settings(
        &validation,
        method,
        request,
        Duration::from_millis(timeout_ms),
        settings,
    )
    .await
}

#[cfg(test)]
pub(crate) async fn try_run_external_adapter(
    params: ExternalAdapterTryRunParams,
) -> InfraResult<ExternalAdapterRunResult> {
    try_run_external_adapter_with_settings(params, &serde_json::json!({})).await
}
