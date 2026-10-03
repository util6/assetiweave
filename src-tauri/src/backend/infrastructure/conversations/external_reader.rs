use super::external::{resolve_source_location_for_adapter, run_external_adapter_read_session};
use super::external_manifest::{
    run_external_adapter_with_settings, source_config_value, validate_external_adapter_for_method,
    validate_external_adapter_manifest, validate_external_adapter_manifest_for_method,
};
use super::external_process::{prepare_adapter_invocation, run_prepared_adapter};
use super::prelude::*;

pub(crate) type ExternalAdapterProgressListener =
    std::sync::Arc<dyn Fn(&ExternalAdapterProgress) + Send + Sync + 'static>;

/// One source sync shares runtime discovery/probing, not mutable adapter output.
/// Revalidate package contents on each call so mid-sync edits still invalidate trust.
pub(crate) struct ExternalAdapterSourceReader<'a> {
    adapter: &'a ConversationAdapter,
    source: &'a ConversationSource,
    invocation: AdapterCommandInvocation,
    content_hash: String,
    cached_validation: ExternalAdapterValidationResult,
    manifest_mtime: Option<std::time::SystemTime>,
    manifest_len: u64,
    executable_mtime: Option<std::time::SystemTime>,
    executable_len: Option<u64>,
    source_value: Value,
    cancellation: Option<&'a tokio_util::sync::CancellationToken>,
    progress_listener: Option<ExternalAdapterProgressListener>,
}

impl<'a> ExternalAdapterSourceReader<'a> {
    pub(crate) async fn new(
        adapter: &'a ConversationAdapter,
        source: &'a ConversationSource,
        settings: &Value,
        cancellation: Option<&'a tokio_util::sync::CancellationToken>,
    ) -> InfraResult<Self> {
        Self::new_for_method(adapter, source, "read_session", settings, cancellation).await
    }

    pub(crate) async fn new_for_method(
        adapter: &'a ConversationAdapter,
        source: &'a ConversationSource,
        method: &str,
        settings: &Value,
        cancellation: Option<&'a tokio_util::sync::CancellationToken>,
    ) -> InfraResult<Self> {
        ensure_read_not_cancelled(cancellation)?;
        validate_external_adapter_for_method(adapter, source, method)?;
        let manifest_path = adapter
            .manifest_path
            .as_deref()
            .ok_or_else(|| InfraError::external("conversation adapter has no manifest"))?;
        let validation = validate_external_adapter_manifest(manifest_path)?;
        validate_external_adapter_manifest_for_method(adapter, &validation, method)?;
        let source_value = json!({
            "location": resolve_source_location_for_adapter(source)?,
            "config": source_config_value(source)?,
        });
        let invocation = prepare_adapter_invocation(&validation, settings).await?;
        let (manifest_mtime, manifest_len) = std::fs::metadata(&validation.manifest_path)
            .ok()
            .map(|m| (m.modified().ok(), m.len()))
            .unwrap_or((None, 0));
        let (executable_mtime, executable_len) = std::fs::metadata(&validation.executable_path)
            .ok()
            .map(|m| (m.modified().ok(), Some(m.len())))
            .unwrap_or((None, None));
        Ok(Self {
            adapter,
            source,
            invocation,
            content_hash: validation.content_hash.clone(),
            cached_validation: validation,
            manifest_mtime,
            manifest_len,
            executable_mtime,
            executable_len,
            source_value,
            cancellation,
            progress_listener: None,
        })
    }

    pub(super) fn with_progress_listener(
        mut self,
        progress_listener: Option<ExternalAdapterProgressListener>,
    ) -> Self {
        self.progress_listener = progress_listener;
        self
    }

    pub(super) async fn read_usage(
        &self,
        mode: &str,
        cursor: Option<&str>,
    ) -> InfraResult<ExternalAdapterRunResult> {
        self.run(
            "read_usage",
            json!({
                "mode": mode,
                "cursor": cursor,
            }),
            DEFAULT_READ_TIMEOUT_MS,
        )
        .await
    }

    pub(super) async fn discover(&self) -> InfraResult<Option<ExternalAdapterRunResult>> {
        if !self
            .adapter
            .capabilities
            .iter()
            .any(|cap| cap == "list_sessions")
        {
            return Ok(None);
        }
        let result = self
            .run(
                "list_sessions",
                json!({"cursor": null}),
                DEFAULT_LIST_TIMEOUT_MS,
            )
            .await?;
        Ok(result.snapshot_complete.then_some(result))
    }

    pub(super) async fn read(
        &self,
        session_id: Option<&str>,
    ) -> InfraResult<ExternalAdapterRunResult> {
        self.read_session(session_id, None).await
    }

    pub(super) async fn read_session(
        &self,
        session_id: Option<&str>,
        source_locator: Option<&str>,
    ) -> InfraResult<ExternalAdapterRunResult> {
        let mut params = json!({"session_id": session_id});
        if let Some(locator) = source_locator {
            params["source_locator"] = json!(locator);
        }
        self.run("read_session", params, DEFAULT_READ_TIMEOUT_MS)
            .await
    }

    pub(crate) async fn run(
        &self,
        method: &str,
        params: Value,
        timeout_ms: u64,
    ) -> InfraResult<ExternalAdapterRunResult> {
        ensure_read_not_cancelled(self.cancellation)?;
        validate_external_adapter_for_method(self.adapter, self.source, method)?;
        let is_modified = {
            let curr_manifest = std::fs::metadata(&self.cached_validation.manifest_path).ok();
            let curr_manifest_mtime = curr_manifest.as_ref().and_then(|m| m.modified().ok());
            let curr_manifest_len = curr_manifest.as_ref().map(|m| m.len()).unwrap_or(0);

            let curr_exec = std::fs::metadata(&self.cached_validation.executable_path).ok();
            let curr_exec_mtime = curr_exec.as_ref().and_then(|m| m.modified().ok());
            let curr_exec_len = curr_exec.as_ref().map(|m| m.len());

            curr_manifest_mtime != self.manifest_mtime
                || curr_manifest_len != self.manifest_len
                || curr_exec_mtime != self.executable_mtime
                || curr_exec_len != self.executable_len
        };
        let validation = if is_modified {
            let revalidated =
                validate_external_adapter_manifest(self.adapter.manifest_path.as_deref().unwrap())?;
            validate_external_adapter_manifest_for_method(self.adapter, &revalidated, method)?;
            if revalidated.content_hash != self.content_hash {
                return Err(InfraError::Conflict(
                    "conversation adapter changed during sync".to_string(),
                ));
            }
            revalidated
        } else {
            self.cached_validation.clone()
        };
        run_prepared_adapter(
            &validation,
            &self.invocation,
            method,
            json!({
                "protocol_version": EXTERNAL_ADAPTER_PROTOCOL_VERSION,
                "request_id": format!("sync-{}-{}", self.source.id, Utc::now().timestamp_millis()),
                "method": method,
                "source": self.source_value,
                "params": params,
            }),
            Duration::from_millis(timeout_ms),
            self.cancellation,
            self.progress_listener.clone(),
        )
        .await
    }
}

pub(super) fn ensure_read_not_cancelled(
    cancellation: Option<&tokio_util::sync::CancellationToken>,
) -> InfraResult<()> {
    if cancellation.is_some_and(tokio_util::sync::CancellationToken::is_cancelled) {
        return Err(InfraError::Cancelled(
            "conversation sync cancelled".to_string(),
        ));
    }
    Ok(())
}

pub(super) async fn read_external_adapter_sessions(
    adapter: &ConversationAdapter,
    source: &ConversationSource,
    settings: &Value,
) -> InfraResult<ExternalAdapterRunResult> {
    run_external_adapter_read_session(adapter, source, None, settings).await
}

#[cfg(test)]
pub(super) async fn discover_external_adapter_sessions(
    adapter: &ConversationAdapter,
    source: &ConversationSource,
    settings: &Value,
) -> InfraResult<Option<ExternalAdapterRunResult>> {
    if !adapter
        .capabilities
        .iter()
        .any(|capability| capability == "list_sessions")
    {
        return Ok(None);
    }
    validate_external_adapter_for_method(adapter, source, "list_sessions")?;
    let manifest_path = adapter.manifest_path.as_deref().ok_or_else(|| {
        InfraError::external(format!(
            "external conversation adapter has no manifest: {}",
            adapter.id
        ))
    })?;
    let validation = validate_external_adapter_manifest(manifest_path)?;
    validate_external_adapter_manifest_for_method(adapter, &validation, "list_sessions")?;
    let source_location = resolve_source_location_for_adapter(source)?;
    let request = json!({
        "protocol_version": EXTERNAL_ADAPTER_PROTOCOL_VERSION,
        "request_id": format!("list-{}-{}", source.id, Utc::now().timestamp_millis()),
        "method": "list_sessions",
        "source": { "location": source_location, "config": source_config_value(source)? },
        "params": { "cursor": null }
    });
    let result = run_external_adapter_with_settings(
        &validation,
        "list_sessions",
        request,
        Duration::from_millis(DEFAULT_LIST_TIMEOUT_MS),
        settings,
    )
    .await?;
    if !result.snapshot_complete {
        return Ok(None);
    }
    Ok(Some(result))
}
