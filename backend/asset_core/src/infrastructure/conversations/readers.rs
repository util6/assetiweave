use super::prelude::*;
use std::collections::{BTreeMap, BTreeSet};

use crate::backend::domain::{
    sanitize_sync_error_message, SessionSyncFailure, SessionSyncWarning, SUBAGENT_EXECUTION_ORIGIN,
};

#[derive(Debug, Clone, Serialize)]
pub struct ConversationSourceReadResult {
    pub sessions: Vec<NormalizedConversationSession>,
    pub session_descriptors: Vec<ConversationSessionDescriptor>,
    pub discovered_session_count: usize,
    pub active_session_count: usize,
    pub skipped_session_count: usize,
    pub legacy_cards_upgraded: usize,
    pub incremental: bool,
    pub session_failures: Vec<SessionSyncFailure>,
    pub session_warnings: Vec<SessionSyncWarning>,
}

#[cfg_attr(not(test), allow(dead_code))]
pub async fn read_source_sessions_with_adapter_with_settings(
    adapter: Option<&ConversationAdapter>,
    source: &ConversationSource,
    settings: &Value,
) -> InfraResult<Vec<NormalizedConversationSession>> {
    let adapter = adapter.ok_or_else(|| {
        InfraError::external(format!(
            "conversation adapter not found: {}",
            source.adapter_id
        ))
    })?;
    read_external_adapter_sessions(adapter, source, settings)
        .await
        .map(|result| result.sessions)
}

pub async fn read_source_sessions_incrementally_with_adapter_with_settings(
    adapter: Option<&ConversationAdapter>,
    source: &ConversationSource,
    known_versions: &BTreeMap<String, String>,
    settings: &Value,
) -> InfraResult<ConversationSourceReadResult> {
    read_source_sessions_with_control(
        adapter,
        source,
        known_versions,
        settings,
        None,
        &mut |_, _| {},
    )
    .await
}

pub async fn read_source_sessions_with_control(
    adapter: Option<&ConversationAdapter>,
    source: &ConversationSource,
    known_versions: &BTreeMap<String, String>,
    settings: &Value,
    cancellation: Option<&tokio_util::sync::CancellationToken>,
    on_progress: &mut (dyn FnMut(usize, usize) + Send),
) -> InfraResult<ConversationSourceReadResult> {
    read_source_sessions_with_progress_listener(
        adapter,
        source,
        known_versions,
        settings,
        cancellation,
        on_progress,
        None,
    )
    .await
}

pub async fn read_source_sessions_with_progress_listener(
    adapter: Option<&ConversationAdapter>,
    source: &ConversationSource,
    known_versions: &BTreeMap<String, String>,
    settings: &Value,
    cancellation: Option<&tokio_util::sync::CancellationToken>,
    on_progress: &mut (dyn FnMut(usize, usize) + Send),
    progress_listener: Option<super::external::ExternalAdapterProgressListener>,
) -> InfraResult<ConversationSourceReadResult> {
    let adapter = adapter.ok_or_else(|| {
        InfraError::external(format!(
            "conversation adapter not found: {}",
            source.adapter_id
        ))
    })?;
    let reader =
        super::external::ExternalAdapterSourceReader::new(adapter, source, settings, cancellation)
            .await?
            .with_progress_listener(progress_listener);
    let Some(discovery) = reader.discover().await? else {
        on_progress(0, 0);
        let result = reader.read(None).await?;
        let sessions = result.sessions;
        on_progress(sessions.len(), sessions.len());
        super::external::ensure_read_not_cancelled(cancellation)?;
        let warnings = result
            .warnings
            .into_iter()
            .map(|w| SessionSyncWarning {
                session_external_id: None,
                code: "adapter_warning".to_string(),
                message: sanitize_with_system_home(&w),
            })
            .collect();
        return Ok(ConversationSourceReadResult {
            session_descriptors: Vec::new(),
            discovered_session_count: sessions.len(),
            active_session_count: sessions.len(),
            skipped_session_count: 0,
            legacy_cards_upgraded: result.legacy_cards_upgraded,
            sessions,
            incremental: false,
            session_failures: Vec::new(),
            session_warnings: warnings,
        });
    };
    let descriptors = deduplicate_session_descriptors(&discovery.session_descriptors)?;
    let active = select_active_session_descriptors(&descriptors, known_versions)?;
    on_progress(0, active.len());
    super::external::ensure_read_not_cancelled(cancellation)?;
    if active.is_empty() {
        let discovered_session_count = descriptors.len();
        return Ok(ConversationSourceReadResult {
            sessions: Vec::new(),
            session_descriptors: descriptors,
            discovered_session_count,
            active_session_count: 0,
            skipped_session_count: discovered_session_count,
            legacy_cards_upgraded: 0,
            incremental: true,
            session_failures: Vec::new(),
            session_warnings: Vec::new(),
        });
    }

    let mut sessions = Vec::with_capacity(active.len());
    let mut session_failures = Vec::new();
    let mut session_warnings = Vec::new();
    let mut empty_session_count = 0usize;
    let mut legacy_cards_upgraded = 0usize;
    for (index, descriptor) in active.iter().enumerate() {
        super::external::ensure_read_not_cancelled(cancellation)?;
        let read_result = reader
            .read_session(
                Some(&descriptor.external_id),
                descriptor.source_locator.as_deref(),
            )
            .await;
        on_progress(index + 1, active.len());
        super::external::ensure_read_not_cancelled(cancellation)?;
        match read_result {
            Ok(result) => {
                legacy_cards_upgraded += result.legacy_cards_upgraded;
                for w in result.warnings {
                    session_warnings.push(SessionSyncWarning {
                        session_external_id: Some(descriptor.external_id.clone()),
                        code: "adapter_warning".to_string(),
                        message: sanitize_with_system_home(&w),
                    });
                }
                let skipped_reason = result
                    .skipped_sessions
                    .iter()
                    .find(|skip| skip.external_id == descriptor.external_id)
                    .map(|skip| skip.reason.clone());
                let mut read = result.sessions;
                if read.is_empty() {
                    empty_session_count += 1;
                    if let Some(reason) = skipped_reason {
                        // 适配器显式声明了跳过原因（子代理轨迹、内部工单等）时保留告警，
                        // 便于排查「会话被结构性跳过、因而永远不进入增量基线」的情况。
                        if reason != "no_turns" {
                            session_warnings.push(SessionSyncWarning {
                                session_external_id: Some(descriptor.external_id.clone()),
                                code: "adapter_skipped_session".to_string(),
                                message: format!("adapter skipped session: {reason}"),
                            });
                        }
                    }
                    continue;
                }
                let mut read = match resolve_read_result(&adapter.id, descriptor, read) {
                    Ok(resolved) => resolved,
                    Err(failure) => {
                        empty_session_count += 1;
                        session_failures.push(failure);
                        continue;
                    }
                };
                sessions.append(&mut read);
            }
            Err(InfraError::Cancelled(msg)) => {
                return Err(InfraError::Cancelled(msg));
            }
            Err(err) => {
                let err_str = err.to_string();
                let lower = err_str.to_ascii_lowercase();
                let (code, stage) = if lower.contains("timed out") || lower.contains("timeout") {
                    ("adapter_timeout", "read")
                } else if lower.contains("exit") || lower.contains("process") {
                    ("adapter_process_error", "adapter")
                } else {
                    ("adapter_error", "read")
                };
                session_failures.push(SessionSyncFailure {
                    session_external_id: descriptor.external_id.clone(),
                    stage: stage.to_string(),
                    error_code: code.to_string(),
                    error_message: sanitize_with_system_home(&err_str),
                    retryable: true,
                });
            }
        }
    }

    let discovered_session_count = descriptors.len();
    let effective_active = active.len().saturating_sub(empty_session_count);
    Ok(ConversationSourceReadResult {
        session_descriptors: descriptors,
        discovered_session_count,
        active_session_count: effective_active,
        skipped_session_count: discovered_session_count.saturating_sub(effective_active),
        legacy_cards_upgraded,
        sessions,
        incremental: true,
        session_failures,
        session_warnings,
    })
}

#[cfg(test)]
pub async fn read_source_sessions_with_adapter(
    adapter: Option<&ConversationAdapter>,
    source: &ConversationSource,
) -> InfraResult<Vec<NormalizedConversationSession>> {
    read_source_sessions_with_adapter_with_settings(adapter, source, &serde_json::json!({})).await
}

#[cfg(test)]
pub async fn read_source_sessions_incrementally_with_adapter(
    adapter: Option<&ConversationAdapter>,
    source: &ConversationSource,
    known_versions: &BTreeMap<String, String>,
) -> InfraResult<ConversationSourceReadResult> {
    read_source_sessions_incrementally_with_adapter_with_settings(
        adapter,
        source,
        known_versions,
        &serde_json::json!({}),
    )
    .await
}

/// 把一次定向读取的结果收口成"唯一根会话 + 它声明的内联子会话"。
///
/// 契约（ADR-0023）：
/// - 必须有且只有一个根会话，且根会话的版本令牌与发现阶段一致（读取期间被改写即失败重试）；
/// - 除根以外的条目必须是子代理会话；无关会话一律拒绝，不做"顺手入库"；
/// - 内联子会话没有独立业务指纹时，用 `父水线:subagent:<子身份>` 派生一个确定性令牌——
///   父文件没动则子会话绝不可能动，因此这个派生值天然可作为下一轮增量比较的凭据。
fn resolve_read_result(
    adapter_id: &str,
    descriptor: &ConversationSessionDescriptor,
    sessions: Vec<NormalizedConversationSession>,
) -> Result<Vec<NormalizedConversationSession>, SessionSyncFailure> {
    let mut seen = BTreeSet::new();
    let mut root_index = Vec::new();
    for (index, session) in sessions.iter().enumerate() {
        if !seen.insert(session.external_id.clone()) {
            return Err(validation_failure(
                descriptor,
                "session_duplicate",
                format!(
                    "conversation adapter {adapter_id} returned duplicate session {}",
                    session.external_id
                ),
            ));
        }
        if session.external_id == descriptor.external_id {
            root_index.push(index);
        }
    }
    let [root_index] = root_index.as_slice() else {
        return Err(validation_failure(
            descriptor,
            "session_mismatch",
            format!(
                "conversation adapter {adapter_id} returned {} root sessions for {}",
                root_index.len(),
                descriptor.external_id
            ),
        ));
    };

    let mut resolved = sessions;
    let derived_prefix = format!("{}:subagent:", descriptor.version_token);
    for index in 0..resolved.len() {
        if index == *root_index {
            if resolved[index].source_fingerprint.as_deref() != Some(&descriptor.version_token) {
                let actual = resolved[index]
                    .source_fingerprint
                    .as_deref()
                    .unwrap_or("<none>");
                return Err(validation_failure(
                    descriptor,
                    "session_version_drift",
                    format!(
                        "session content no longer matches the discovered version (expected {}, got {})",
                        descriptor.version_token, actual
                    ),
                ));
            }
            continue;
        }
        if resolved[index].execution_origin.as_deref() != Some(SUBAGENT_EXECUTION_ORIGIN) {
            return Err(validation_failure(
                descriptor,
                "session_unrelated",
                format!(
                    "conversation adapter {adapter_id} returned unrelated session {} for {}",
                    resolved[index].external_id, descriptor.external_id
                ),
            ));
        }
        let child_token = format!("{derived_prefix}{}", resolved[index].external_id);
        let has_native_version = resolved[index]
            .source_fingerprint
            .as_deref()
            .map(str::trim)
            .is_some_and(|value| !value.is_empty());
        if !has_native_version {
            // 内联子会话没有独立指纹：确定性继承父水线，杜绝"每轮必重导"的僵尸变动。
            // 有独立业务版本（例如子代理自己有落盘文件）时一律采信适配器给的原生令牌：
            // 那是另一个物理来源的水线，不可能也不应该等于父文件令牌。
            resolved[index].source_fingerprint = Some(child_token);
        }
    }
    Ok(resolved)
}

fn validation_failure(
    descriptor: &ConversationSessionDescriptor,
    code: &str,
    message: String,
) -> SessionSyncFailure {
    SessionSyncFailure {
        session_external_id: descriptor.external_id.clone(),
        stage: "validate".to_string(),
        error_code: code.to_string(),
        error_message: sanitize_with_system_home(&message),
        retryable: true,
    }
}

fn deduplicate_session_descriptors(
    descriptors: &[ConversationSessionDescriptor],
) -> InfraResult<Vec<ConversationSessionDescriptor>> {
    let mut seen = BTreeMap::<String, ConversationSessionDescriptor>::new();
    for descriptor in descriptors {
        if let Some(existing) = seen.get(&descriptor.external_id) {
            if existing.version_token != descriptor.version_token
                || existing.source_locator != descriptor.source_locator
            {
                return Err(InfraError::external(format!(
                    "conversation adapter returned conflicting descriptors for session {}",
                    descriptor.external_id
                )));
            }
            continue;
        }
        seen.insert(descriptor.external_id.clone(), descriptor.clone());
    }
    Ok(seen.into_values().collect())
}

fn select_active_session_descriptors(
    descriptors: &[ConversationSessionDescriptor],
    known_versions: &BTreeMap<String, String>,
) -> InfraResult<Vec<ConversationSessionDescriptor>> {
    let descriptors = deduplicate_session_descriptors(descriptors)?;
    let mut active_ids = BTreeSet::new();
    Ok(descriptors
        .into_iter()
        .filter(|descriptor| {
            let changed = known_versions
                .get(&descriptor.external_id)
                .is_none_or(|known| known != &descriptor.version_token);
            changed && active_ids.insert(descriptor.external_id.clone())
        })
        .collect())
}

fn sanitize_with_system_home(message: &str) -> String {
    let mut home_paths = Vec::new();
    let home = std::env::var("HOME").ok();
    let userprofile = std::env::var("USERPROFILE").ok();
    if let Some(h) = home.as_deref() {
        home_paths.push(h);
    }
    if let Some(u) = userprofile.as_deref() {
        home_paths.push(u);
    }
    sanitize_sync_error_message(message, &home_paths)
}

#[cfg(test)]
#[path = "readers_tests.rs"]
mod tests;
