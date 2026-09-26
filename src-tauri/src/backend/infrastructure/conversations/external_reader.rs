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
        Ok(Self {
            adapter,
            source,
            invocation,
            content_hash: validation.content_hash,
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
        self.run(
            "read_session",
            json!({"session_id": session_id}),
            DEFAULT_READ_TIMEOUT_MS,
        )
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
        let validation =
            validate_external_adapter_manifest(self.adapter.manifest_path.as_deref().unwrap())?;
        validate_external_adapter_manifest_for_method(self.adapter, &validation, method)?;
        if validation.content_hash != self.content_hash {
            return Err(InfraError::Conflict(
                "conversation adapter changed during sync".to_string(),
            ));
        }
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
