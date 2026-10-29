use super::*;

#[derive(Debug, FromRow)]
pub struct WebRecordSessionListItemRow {
    pub id: String,
    pub source_id: String,
    pub adapter_id: String,
    pub external_id: String,
    pub title: String,
    pub project_path: Option<String>,
    pub started_at: Option<String>,
    pub updated_at: Option<String>,
    pub source_locator: Option<String>,
    pub source_fingerprint: Option<String>,
    pub missing: i64,
    pub created_at: String,
    pub imported_at: String,
    pub question_count: i64,
    pub turn_count: i64,
}

#[derive(Debug, FromRow)]
pub struct ExistingWebRecordSessionRow {
    pub title: String,
    pub started_at: Option<String>,
    pub updated_at: Option<String>,
    pub source_locator: Option<String>,
    pub source_fingerprint: Option<String>,
    pub missing: i64,
}

#[derive(Debug, FromRow)]
pub struct ExistingWebRecordTurnRow {
    pub external_id: String,
    pub fingerprint: String,
    pub missing: i64,
}

pub fn web_record_session_from_normalized(
    source: &ConversationSource,
    normalized: &NormalizedConversationSession,
    now: &str,
) -> ConversationSession {
    ConversationSession {
        id: stable_id("web-record-session", &[&source.id, &normalized.external_id]),
        source_id: source.id.clone(),
        adapter_id: source.adapter_id.clone(),
        external_id: normalized.external_id.clone(),
        title: normalized
            .title
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("Untitled web conversation")
            .to_string(),
        project_path: None,
        started_at: normalized.started_at.clone(),
        updated_at: normalized.updated_at.clone(),
        source_locator: normalized.source_locator.clone(),
        source_fingerprint: normalized.source_fingerprint.clone(),
        missing: false,
        created_at: now.to_string(),
        imported_at: now.to_string(),
        execution_origin: "user".to_string(),
        execution_purpose: None,
        user_visible: true,
        last_cwd: None,
    }
}

pub fn web_record_turn_from_normalized(
    session_id: &str,
    normalized: &crate::backend::domain::NormalizedConversationTurn,
    now: &str,
) -> ConversationTurn {
    let parent_turn_id = normalized
        .parent_external_id
        .as_deref()
        .map(|parent_ext_id| stable_id("web-record-turn", &[session_id, parent_ext_id]));
    let has_sparse_meta = normalized.title.is_some()
        || normalized.turn_kind.is_some()
        || normalized.agent_role.is_some()
        || parent_turn_id.is_some();
    let process = if has_sparse_meta {
        let p = crate::backend::domain::conversations::triad::TurnProcess {
            title: normalized.title.clone(),
            turn_kind: normalized.turn_kind.clone(),
            agent_role: normalized.agent_role.clone(),
            parent_turn_id: parent_turn_id.clone(),
            ..Default::default()
        };
        serde_json::to_string(&p).ok()
    } else {
        None
    };
    ConversationTurn {
        id: stable_id("web-record-turn", &[session_id, &normalized.external_id]),
        session_id: session_id.to_string(),
        external_id: normalized.external_id.clone(),
        turn_index: normalized.turn_index,
        prompt: normalized.user_text.trim().to_string(),
        title: normalized.title.clone(),
        started_at: normalized.started_at.clone(),
        ended_at: normalized.ended_at.clone(),
        fingerprint: conversation_turn_fingerprint(normalized),
        missing: false,
        imported_at: now.to_string(),
        model: normalized.model.clone(),
        process,
        response: None,
        turn_kind: normalized.turn_kind.clone(),
        agent_role: normalized.agent_role.clone(),
        parent_turn_id,
    }
}

pub async fn prepare_web_record_turn_for_import_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    session_id: &str,
    normalized: &crate::backend::domain::NormalizedConversationTurn,
    now: &str,
    translation_state: &BTreeMap<String, (Option<String>, Option<String>, Option<String>)>,
) -> StoreResult<(
    ConversationTurn,
    Vec<crate::backend::domain::conversations::triad::TriadPart>,
)> {
    let mut triad = crate::backend::domain::conversations::triad::split_normalized_turn(
        "web",
        &normalized.user_text,
        &normalized.parts,
    );

    let old_row = sqlx::query_as::<_, (Option<String>, Option<String>)>(
        "SELECT process, response FROM web_record_turns WHERE tenant_id = ?1 AND session_id = ?2 AND external_id = ?3",
    )
    .bind(tenant_id)
    .bind(session_id)
    .bind(&normalized.external_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(StoreError::external)?;

    let mut old_process: Option<crate::backend::domain::conversations::triad::TurnProcess> = None;
    if let Some((Some(old_process_json), old_response)) = old_row {
        if let Ok(parsed_old) = serde_json::from_str::<
            crate::backend::domain::conversations::triad::TurnProcess,
        >(&old_process_json)
        {
            if old_response.as_deref() == triad.response.as_deref() {
                if let Some(ref mut new_p) = triad.process {
                    if new_p.response_translated_text.is_none() {
                        new_p.response_translated_text =
                            parsed_old.response_translated_text.clone();
                    }
                } else if let Some(ref old_trans) = parsed_old.response_translated_text {
                    let p = triad.process.get_or_insert_with(Default::default);
                    p.response_translated_text = Some(old_trans.clone());
                }
            }
            old_process = Some(parsed_old);
        }
    }

    let turn_id = stable_id("web-record-turn", &[session_id, &normalized.external_id]);
    if let Some(ref resp_text) = triad.response {
        let resp_seq = triad
            .process
            .as_ref()
            .and_then(|p| p.response_seq)
            .unwrap_or(0);
        let resp_part_id =
            crate::backend::domain::derive_standard_part_id("web-record-part", &turn_id, resp_seq);
        if let Some((old_content, _, translated_text)) = translation_state.get(&resp_part_id) {
            if old_content.as_deref() == Some(resp_text.as_str()) {
                if let Some(ref mut p) = triad.process {
                    if p.response_translated_text.is_none() {
                        p.response_translated_text = translated_text.clone();
                    }
                } else {
                    triad.process =
                        Some(crate::backend::domain::conversations::triad::TurnProcess {
                            response_seq: Some(resp_seq),
                            response_translated_text: translated_text.clone(),
                            ..Default::default()
                        });
                }
            }
        }
    }

    let parent_turn_id = normalized
        .parent_external_id
        .as_deref()
        .map(|parent_ext_id| stable_id("web-record-turn", &[session_id, parent_ext_id]))
        .or_else(|| old_process.as_ref().and_then(|p| p.parent_turn_id.clone()));
    let title = normalized
        .title
        .clone()
        .or_else(|| old_process.as_ref().and_then(|p| p.title.clone()));
    let turn_kind = normalized
        .turn_kind
        .clone()
        .or_else(|| old_process.as_ref().and_then(|p| p.turn_kind.clone()));
    let agent_role = normalized
        .agent_role
        .clone()
        .or_else(|| old_process.as_ref().and_then(|p| p.agent_role.clone()));

    let has_sparse_meta =
        title.is_some() || turn_kind.is_some() || agent_role.is_some() || parent_turn_id.is_some();

    if has_sparse_meta {
        let p = triad.process.get_or_insert_with(Default::default);
        if title.is_some() {
            p.title = title.clone();
        }
        if turn_kind.is_some() {
            p.turn_kind = turn_kind.clone();
        }
        if agent_role.is_some() {
            p.agent_role = agent_role.clone();
        }
        if parent_turn_id.is_some() {
            p.parent_turn_id = parent_turn_id.clone();
        }
    }

    let process_json = triad
        .process
        .as_ref()
        .filter(|p| !p.is_empty())
        .map(|p| serde_json::to_string(p).unwrap_or_default());
    let turn = ConversationTurn {
        id: turn_id,
        session_id: session_id.to_string(),
        external_id: normalized.external_id.clone(),
        turn_index: normalized.turn_index,
        prompt: triad.prompt,
        title,
        started_at: normalized.started_at.clone(),
        ended_at: normalized.ended_at.clone(),
        fingerprint: conversation_turn_fingerprint(normalized),
        missing: false,
        imported_at: now.to_string(),
        model: normalized.model.clone(),
        process: process_json,
        response: triad.response,
        turn_kind,
        agent_role,
        parent_turn_id,
    };

    Ok((turn, triad.parts))
}

pub fn normalize_query(query: Option<&str>) -> Option<String> {
    query
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_lowercase)
}

pub fn first_line(text: &str) -> String {
    let line = text
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("Untitled question");
    let trimmed = line.trim();
    if trimmed.chars().count() > 96 {
        trimmed.chars().take(96).collect()
    } else {
        trimmed.to_string()
    }
}

pub fn stable_id(prefix: &str, parts: &[&str]) -> String {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update(part.as_bytes());
        hasher.update(b"\0");
    }
    format!("{prefix}-{:x}", hasher.finalize())
}
