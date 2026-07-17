use super::prelude::*;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Serialize)]
pub(crate) struct ConversationSourceReadResult {
    pub(crate) sessions: Vec<NormalizedConversationSession>,
    pub(crate) session_descriptors: Vec<ConversationSessionDescriptor>,
    pub(crate) discovered_session_count: usize,
    pub(crate) active_session_count: usize,
    pub(crate) skipped_session_count: usize,
    pub(crate) incremental: bool,
}

#[cfg_attr(not(test), allow(dead_code))]
pub(crate) fn read_source_sessions_with_adapter(
    adapter: Option<&ConversationAdapter>,
    source: &ConversationSource,
) -> AppResult<Vec<NormalizedConversationSession>> {
    let adapter =
        adapter.ok_or_else(|| format!("conversation adapter not found: {}", source.adapter_id))?;
    read_external_adapter_sessions(adapter, source)
}

pub(crate) fn read_source_sessions_incrementally_with_adapter(
    adapter: Option<&ConversationAdapter>,
    source: &ConversationSource,
    known_versions: &BTreeMap<String, String>,
) -> AppResult<ConversationSourceReadResult> {
    let adapter =
        adapter.ok_or_else(|| format!("conversation adapter not found: {}", source.adapter_id))?;
    let Some(discovery) = discover_external_adapter_sessions(adapter, source)? else {
        let sessions = read_external_adapter_sessions(adapter, source)?;
        return Ok(ConversationSourceReadResult {
            session_descriptors: Vec::new(),
            discovered_session_count: sessions.len(),
            active_session_count: sessions.len(),
            skipped_session_count: 0,
            sessions,
            incremental: false,
        });
    };
    let descriptors = deduplicate_session_descriptors(&discovery.session_descriptors)?;
    let active = select_active_session_descriptors(&descriptors, known_versions)?;
    if active.is_empty() {
        let discovered_session_count = descriptors.len();
        return Ok(ConversationSourceReadResult {
            sessions: Vec::new(),
            session_descriptors: descriptors,
            discovered_session_count,
            active_session_count: 0,
            skipped_session_count: discovered_session_count,
            incremental: true,
        });
    }

    let (sessions, empty_session_count) = if known_versions.is_empty()
        && active.len() == descriptors.len()
    {
        (read_external_adapter_sessions(adapter, source)?, 0usize)
    } else {
        let mut sessions = Vec::with_capacity(active.len());
        let mut empty_count = 0usize;
        for descriptor in &active {
            let mut read = read_external_adapter_session(adapter, source, &descriptor.external_id)?;
            if read.is_empty() {
                // Session was discovered by list_sessions but has no readable
                // content yet (e.g. an active session that just started and has
                // no complete turns).  Skip it — the next sync will pick it up
                // once content is available.
                empty_count += 1;
                continue;
            }
            if read.len() != 1 || read[0].external_id != descriptor.external_id {
                return Err(format!(
                    "conversation adapter {} returned {} sessions for active session {}",
                    adapter.id,
                    read.len(),
                    descriptor.external_id
                ));
            }
            sessions.append(&mut read);
        }
        (sessions, empty_count)
    };

    let descriptors_by_id = descriptors
        .iter()
        .map(|descriptor| (descriptor.external_id.as_str(), descriptor))
        .collect::<BTreeMap<_, _>>();
    for session in &sessions {
        let descriptor = descriptors_by_id
            .get(session.external_id.as_str())
            .ok_or_else(|| {
                format!(
                    "conversation adapter {} returned undiscovered session {}",
                    adapter.id, session.external_id
                )
            })?;
        validate_session_matches_descriptor(session, descriptor)?;
    }

    let discovered_session_count = descriptors.len();
    let effective_active = active.len().saturating_sub(empty_session_count);
    Ok(ConversationSourceReadResult {
        session_descriptors: descriptors,
        discovered_session_count,
        active_session_count: effective_active,
        skipped_session_count: discovered_session_count.saturating_sub(effective_active),
        sessions,
        incremental: true,
    })
}

fn validate_session_matches_descriptor(
    session: &NormalizedConversationSession,
    descriptor: &ConversationSessionDescriptor,
) -> AppResult<()> {
    if session.source_fingerprint.as_deref() != Some(descriptor.version_token.as_str()) {
        return Err(format!(
            "conversation session {} changed while it was being read; expected version {}, got {}",
            descriptor.external_id,
            descriptor.version_token,
            session.source_fingerprint.as_deref().unwrap_or("<missing>")
        ));
    }
    Ok(())
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
                return Err(format!(
                    "conversation adapter returned conflicting descriptors for session {}",
                    descriptor.external_id
                ));
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

        let error = validate_session_matches_descriptor(&session, &descriptor)
            .expect_err("content read from another version must remain dirty");

        assert!(error.contains("changed while it was being read"));
    }
}
