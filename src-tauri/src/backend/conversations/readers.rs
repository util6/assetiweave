use super::prelude::*;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ConversationSourceReadResult {
    pub(crate) sessions: Vec<NormalizedConversationSession>,
    pub(crate) session_descriptors: Vec<ConversationSessionDescriptor>,
    pub(crate) discovered_session_count: usize,
    pub(crate) active_session_count: usize,
    pub(crate) skipped_session_count: usize,
    pub(crate) legacy_cards_upgraded: usize,
    pub(crate) incremental: bool,
}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn read_source_sessions_with_adapter_with_settings(
    adapter: Option<&ConversationAdapter>,
    source: &ConversationSource,
    settings: &Value,
) -> AppResult<Vec<NormalizedConversationSession>> {
    let adapter = adapter.ok_or_else(|| {
        AppError::external({ format!("conversation adapter not found: {}", source.adapter_id) })
    })?;
    read_external_adapter_sessions(adapter, source, settings).map(|result| result.sessions)
}

pub(crate) fn read_source_sessions_incrementally_with_adapter_with_settings(
    adapter: Option<&ConversationAdapter>,
    source: &ConversationSource,
    known_versions: &BTreeMap<String, String>,
    settings: &Value,
) -> AppResult<ConversationSourceReadResult> {
    read_source_sessions_with_control(
        adapter,
        source,
        known_versions,
        settings,
        None,
        &mut |_, _| {},
    )
}

pub(crate) fn read_source_sessions_with_control(
    adapter: Option<&ConversationAdapter>,
    source: &ConversationSource,
    known_versions: &BTreeMap<String, String>,
    settings: &Value,
    cancellation: Option<&tokio_util::sync::CancellationToken>,
    on_progress: &mut dyn FnMut(usize, usize),
) -> AppResult<ConversationSourceReadResult> {
    let adapter = adapter.ok_or_else(|| {
        AppError::external({ format!("conversation adapter not found: {}", source.adapter_id) })
    })?;
    let reader =
        super::external::ExternalAdapterSourceReader::new(adapter, source, settings, cancellation)?;
    let Some(discovery) = reader.discover()? else {
        on_progress(0, 0);
        let result = reader.read(None)?;
        let sessions = result.sessions;
        on_progress(sessions.len(), sessions.len());
        super::external::ensure_read_not_cancelled(cancellation)?;
        return Ok(ConversationSourceReadResult {
            session_descriptors: Vec::new(),
            discovered_session_count: sessions.len(),
            active_session_count: sessions.len(),
            skipped_session_count: 0,
            legacy_cards_upgraded: result.legacy_cards_upgraded,
            sessions,
            incremental: false,
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
        });
    }

    let mut sessions = Vec::with_capacity(active.len());
    let mut empty_session_count = 0usize;
    let mut legacy_cards_upgraded = 0usize;
    for (index, descriptor) in active.iter().enumerate() {
        let result = reader.read(Some(&descriptor.external_id))?;
        on_progress(index + 1, active.len());
        super::external::ensure_read_not_cancelled(cancellation)?;
        legacy_cards_upgraded += result.legacy_cards_upgraded;
        let mut read = result.sessions;
        if read.is_empty() {
            // Session was discovered by list_sessions but has no readable
            // content yet (e.g. an active session that just started and has
            // no complete turns). Skip it — the next sync will pick it up
            // once content is available.
            empty_session_count += 1;
            continue;
        }
        if read.len() != 1 || read[0].external_id != descriptor.external_id {
            return Err(AppError::external(format!(
                "conversation adapter {} returned {} sessions for active session {}",
                adapter.id,
                read.len(),
                descriptor.external_id
            )));
        }
        if !session_matches_descriptor(&read[0], descriptor) {
            // Discovery and hydration are separate adapter calls. A live
            // session can advance between them, so importing this snapshot
            // would pair new content with a stale discovery token. Leave it
            // dirty and let the next incremental sync retry it instead of
            // failing the entire source.
            empty_session_count += 1;
            continue;
        }
        sessions.append(&mut read);
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
    })
}

#[cfg(test)]
pub(crate) fn read_source_sessions_with_adapter(
    adapter: Option<&ConversationAdapter>,
    source: &ConversationSource,
) -> AppResult<Vec<NormalizedConversationSession>> {
    read_source_sessions_with_adapter_with_settings(adapter, source, &serde_json::json!({}))
}

#[cfg(test)]
pub(crate) fn read_source_sessions_incrementally_with_adapter(
    adapter: Option<&ConversationAdapter>,
    source: &ConversationSource,
    known_versions: &BTreeMap<String, String>,
) -> AppResult<ConversationSourceReadResult> {
    read_source_sessions_incrementally_with_adapter_with_settings(
        adapter,
        source,
        known_versions,
        &serde_json::json!({}),
    )
}

fn session_matches_descriptor(
    session: &NormalizedConversationSession,
    descriptor: &ConversationSessionDescriptor,
) -> bool {
    session.source_fingerprint.as_deref() == Some(descriptor.version_token.as_str())
}

fn deduplicate_session_descriptors(
    descriptors: &[ConversationSessionDescriptor],
) -> AppResult<Vec<ConversationSessionDescriptor>> {
    let mut seen = BTreeMap::<String, ConversationSessionDescriptor>::new();
    for descriptor in descriptors {
        if let Some(existing) = seen.get(&descriptor.external_id) {
            if existing.version_token != descriptor.version_token
                || existing.source_locator != descriptor.source_locator
            {
                return Err(AppError::external(format!(
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
) -> AppResult<Vec<ConversationSessionDescriptor>> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn conversation_incremental_selects_old_session_when_its_version_changes() {
        let descriptors = vec![
            ConversationSessionDescriptor {
                external_id: "old-session".to_string(),
                updated_at: Some("2026-07-16T01:02:03Z".to_string()),
                source_locator: Some("/tmp/old.jsonl".to_string()),
                version_token: "version-2".to_string(),
            },
            ConversationSessionDescriptor {
                external_id: "unchanged-session".to_string(),
                updated_at: Some("2026-07-15T01:02:03Z".to_string()),
                source_locator: Some("/tmp/unchanged.jsonl".to_string()),
                version_token: "same-version".to_string(),
            },
        ];
        let known_versions = BTreeMap::from([
            ("old-session".to_string(), "version-1".to_string()),
            ("unchanged-session".to_string(), "same-version".to_string()),
        ]);

        let active = select_active_session_descriptors(&descriptors, &known_versions)
            .expect("select active sessions");

        assert_eq!(active.len(), 1);
        assert_eq!(active[0].external_id, "old-session");
    }

    #[test]
    fn conversation_incremental_rejects_conflicting_duplicate_descriptors() {
        let descriptors = vec![
            ConversationSessionDescriptor {
                external_id: "duplicate".to_string(),
                updated_at: None,
                source_locator: None,
                version_token: "version-1".to_string(),
            },
            ConversationSessionDescriptor {
                external_id: "duplicate".to_string(),
                updated_at: None,
                source_locator: None,
                version_token: "version-2".to_string(),
            },
        ];

        let error = select_active_session_descriptors(&descriptors, &BTreeMap::new())
            .expect_err("conflicting versions must fail discovery");

        assert!(error.contains("duplicate"));
    }

    #[test]
    fn conversation_incremental_rejects_content_from_a_different_version() {
        let descriptor = ConversationSessionDescriptor {
            external_id: "session-1".to_string(),
            updated_at: None,
            source_locator: None,
            version_token: "version-before-read".to_string(),
        };
        let session = NormalizedConversationSession {
            external_id: "session-1".to_string(),
            title: None,
            project_path: None,
            started_at: None,
            updated_at: None,
            source_locator: None,
            source_fingerprint: Some("version-after-read".to_string()),
            turns: Vec::new(),
        };

        assert!(!session_matches_descriptor(&session, &descriptor));
    }
}
