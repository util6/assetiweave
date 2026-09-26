use super::external_process::{prepare_adapter_invocation, run_prepared_adapter};
use super::prelude::*;

pub(crate) fn validate_external_adapter_for_method(
    adapter: &ConversationAdapter,
    source: &ConversationSource,
    method: &str,
) -> InfraResult<()> {
    if adapter.kind != ConversationAdapterKind::External {
        return Err(InfraError::external(format!(
            "conversation adapter {} is not a built-in or external adapter",
            adapter.id
        )));
    }
    if !adapter.enabled {
        return Err(InfraError::external(format!(
            "conversation adapter is disabled: {}",
            adapter.id
        )));
    }
    if !matches!(
        adapter.trust_state,
        ConversationAdapterTrustState::Trusted | ConversationAdapterTrustState::BuiltIn
    ) {
        return Err(InfraError::external(format!(
            "external conversation adapter is not trusted: {}",
            adapter.id
        )));
    }
    if !adapter.input_kinds.iter().any(|kind| *kind == source.kind) {
        return Err(InfraError::external(format!(
            "external conversation adapter {} does not support source kind {:?}",
            adapter.id, source.kind
        )));
    }
    if !adapter
        .capabilities
        .iter()
        .any(|capability| capability == method)
    {
        return Err(InfraError::external(format!(
            "external conversation adapter {} does not declare {method}",
            adapter.id
        )));
    }
    Ok(())
}

pub(crate) fn validate_external_adapter_manifest_for_method(
    adapter: &ConversationAdapter,
    validation: &ExternalAdapterValidationResult,
    method: &str,
) -> InfraResult<()> {
    if validation.manifest.id != adapter.id {
        return Err(InfraError::external(format!(
            "external conversation adapter manifest id {} does not match registered adapter {}",
            validation.manifest.id, adapter.id
        )));
    }
    if !validation
        .manifest
        .capabilities
        .iter()
        .any(|capability| capability == method)
    {
        return Err(InfraError::external(format!(
            "external conversation adapter {} does not declare {method}",
            adapter.id
        )));
    }
    if let Some(trusted_hash) = adapter.trusted_hash.as_deref() {
        if validation.content_hash != trusted_hash {
            return Err(InfraError::external(format!(
                "external conversation adapter trusted hash mismatch: {}",
                adapter.id
            )));
        }
    }
    Ok(())
}

pub(crate) fn source_config_value(source: &ConversationSource) -> InfraResult<Option<Value>> {
    match source.config_json.as_deref() {
        Some(text) if !text.trim().is_empty() => Ok(Some(
            serde_json::from_str::<Value>(text).map_err(InfraError::external)?,
        )),
        _ => Ok(None),
    }
}

pub(crate) fn adapter_from_registration_preview(value: Value) -> InfraResult<ConversationAdapter> {
    let adapter = value
        .get("adapter")
        .cloned()
        .ok_or_else(|| InfraError::external("registration preview did not include adapter"))?;
    serde_json::from_value(adapter).map_err(InfraError::external)
}

pub(crate) async fn list_conversation_adapter_runtime_statuses_with_settings(
    adapters: &[ConversationAdapter],
    sources: &[ConversationSource],
    settings: &Value,
) -> InfraResult<Vec<ConversationAdapterRuntimeStatus>> {
    let mut requirements = adapter_runtime_requirements(adapters);
    super::harvester::append_harvester_runtime_requirements(&mut requirements, sources);
    Ok(list_adapter_runtime_statuses_with_settings(&requirements, settings).await)
}

pub(crate) fn validate_external_adapter_manifest(
    manifest_path: &str,
) -> InfraResult<ExternalAdapterValidationResult> {
    let path = crate::backend::infrastructure::path_utils::expand_path(manifest_path)?;
    if !path.is_file() {
        return Err(InfraError::external(format!(
            "adapter manifest not found: {}",
            path.display()
        )));
    }
    let manifest_text = fs::read_to_string(&path).map_err(InfraError::external)?;
    let manifest: ConversationAdapterManifest =
        serde_json::from_str(&manifest_text).map_err(InfraError::external)?;
    validate_manifest_shape(&manifest)?;
    let manifest_dir = path
        .parent()
        .ok_or_else(|| InfraError::external("adapter manifest path has no parent directory"))?;
    let executable_path = resolve_adapter_entry_path(manifest_dir, &manifest)?;
    let executable_hash = if executable_path.is_file() {
        Some(hash_file(&executable_path)?)
    } else {
        None
    };
    let manifest_hash = hash_bytes(manifest_text.as_bytes());
    let content_hash = adapter_content_hash(&manifest_hash, executable_hash.as_deref());
    let mut warnings = Vec::new();
    if executable_hash.is_none() {
        warnings.push(format!(
            "executable does not exist locally or cannot be hashed: {}",
            executable_path.display()
        ));
    }
    Ok(ExternalAdapterValidationResult {
        valid: true,
        manifest_path: path.to_string_lossy().to_string(),
        content_hash,
        manifest_hash,
        executable_path: executable_path.to_string_lossy().to_string(),
        executable_hash,
        manifest,
        warnings,
    })
}

pub(crate) fn adapter_content_hash(manifest_hash: &str, executable_hash: Option<&str>) -> String {
    let executable_hash = executable_hash.unwrap_or("");
    hash_bytes(format!("manifest:{manifest_hash}\nexecutable:{executable_hash}").as_bytes())
}

pub(crate) fn validate_manifest_shape(manifest: &ConversationAdapterManifest) -> InfraResult<()> {
    if manifest.schema_version != 1 {
        return Err(InfraError::external(
            "adapter schema_version must be 1".to_string(),
        ));
    }
    if manifest.protocol_version != EXTERNAL_ADAPTER_PROTOCOL_VERSION {
        return Err(InfraError::external(format!(
            "adapter protocol_version must be {EXTERNAL_ADAPTER_PROTOCOL_VERSION}"
        )));
    }
    if manifest.id.trim().is_empty() {
        return Err(InfraError::external("adapter id is required".to_string()));
    }
    if manifest.name.trim().is_empty() {
        return Err(InfraError::external("adapter name is required".to_string()));
    }
    crate::backend::domain::conversations::projection::validate_manifest_card_kinds(
        &manifest.id,
        manifest.card_contract_version,
        &manifest.card_kinds,
    )
    .map_err(InfraError::external)?;
    if manifest.runtime.is_some() && !manifest.command.is_empty() {
        return Err(InfraError::external(
            "adapter manifest must not declare both runtime and command".to_string(),
        ));
    }
    match manifest.runtime.as_ref() {
        Some(runtime) => {
            if runtime.entry.trim().is_empty() {
                return Err(InfraError::external(
                    "adapter runtime entry is required".to_string(),
                ));
            }
            validate_adapter_entry_path("adapter runtime entry", &runtime.entry)?;
            if runtime
                .version
                .as_deref()
                .is_some_and(|version| version.trim().is_empty())
            {
                return Err(InfraError::external(
                    "adapter runtime version must not be empty".to_string(),
                ));
            }
            if let Some(version) = runtime.version.as_deref() {
                validate_runtime_version_constraint(version)?;
            }
        }
        None if manifest.command.is_empty() || manifest.command[0].trim().is_empty() => {
            return Err(InfraError::external(
                "adapter command must include an executable".to_string(),
            ));
        }
        None => validate_adapter_entry_path("adapter command", &manifest.command[0])?,
    }
    for capability in &manifest.capabilities {
        if !matches!(
            capability.as_str(),
            "probe"
                | "list_sessions"
                | "read_session"
                | "export_markdown"
                | "web_records"
                | "project_command_parts"
                | "read_usage"
        ) {
            return Err(InfraError::external(format!(
                "unsupported adapter capability: {capability}"
            )));
        }
    }
    Ok(())
}

pub(crate) fn validate_adapter_entry_path(field: &str, raw: &str) -> InfraResult<()> {
    validate_conversation_adapter_entry_path(raw).map_err(|error| match error {
        ConversationAdapterEntryPathError::Rooted => InfraError::external(format!(
            "{field} must be a relative path inside the adapter directory"
        )),
        ConversationAdapterEntryPathError::ParentTraversal => {
            InfraError::external(format!("{field} must not escape the adapter directory"))
        }
    })
}

pub(crate) async fn run_external_adapter_with_settings(
    validation: &ExternalAdapterValidationResult,
    method: &str,
    request: Value,
    timeout: Duration,
    settings: &Value,
) -> InfraResult<ExternalAdapterRunResult> {
    let invocation = prepare_adapter_invocation(validation, settings).await?;
    run_prepared_adapter(
        validation,
        &invocation,
        method,
        request,
        timeout,
        None,
        None,
    )
    .await
}
