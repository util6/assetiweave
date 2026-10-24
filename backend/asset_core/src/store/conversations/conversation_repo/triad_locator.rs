use super::projection::{
    conversation_card_block_locator, load_conversation_card_projection_context_for_record_sqlx,
};
use super::*;
use crate::backend::domain::conversations::triad::{TurnProcess, TurnProcessItem};
use crate::backend::domain::conversations::{
    derive_conversation_part_id, derive_part_id_key, derive_standard_part_id,
    ConversationBlockDetail, ConversationCardRenderer, ConversationContentCardDescriptor,
    ConversationPart, ConversationPartKind, ConversationPartRole, ConversationRecordKind,
};

#[derive(Debug, Clone)]
pub struct TriadPartCandidate {
    pub full_id: String,
    pub session_id: String,
    pub turn_id: String,
    pub part_index: i64,
    pub is_web: bool,
    pub is_response: bool,
}

pub async fn query_triad_parts_by_prefix(
    pool: &SqlitePool,
    tenant_id: &str,
    clean_hash: &str,
    allow_session: bool,
    allow_web: bool,
) -> StoreResult<Vec<TriadPartCandidate>> {
    let mut candidates = Vec::new();
    let lower_clean = clean_hash.to_ascii_lowercase();

    if allow_session {
        let rows = sqlx::query_as::<_, (String, String, Option<String>, Option<String>)>(
            "SELECT session_id, id, process, response FROM conversation_turns WHERE tenant_id = ?1 AND (process IS NOT NULL OR response IS NOT NULL)",
        )
        .bind(tenant_id)
        .fetch_all(pool)
        .await
        .map_err(StoreError::external)?;

        for (session_id, turn_id, process_json, response_text) in rows {
            let process: Option<TurnProcess> = process_json
                .as_deref()
                .and_then(|s| serde_json::from_str(s).ok());

            if let Some(ref p) = process {
                for item in &p.items {
                    let full_id = derive_conversation_part_id(&turn_id, item.seq);
                    if matches_hash_or_key(&full_id, &lower_clean) {
                        candidates.push(TriadPartCandidate {
                            full_id,
                            session_id: session_id.clone(),
                            turn_id: turn_id.clone(),
                            part_index: item.seq,
                            is_web: false,
                            is_response: false,
                        });
                    }
                }
            }

            if let Some(ref resp) = response_text {
                if !resp.trim().is_empty() {
                    let max_seq = process
                        .as_ref()
                        .and_then(|p| p.items.iter().map(|i| i.seq).max())
                        .unwrap_or(-1);
                    let resp_seq = process
                        .as_ref()
                        .and_then(|p| p.response_seq)
                        .unwrap_or(max_seq + 1);
                    let full_id = derive_conversation_part_id(&turn_id, resp_seq);
                    if matches_hash_or_key(&full_id, &lower_clean) {
                        candidates.push(TriadPartCandidate {
                            full_id,
                            session_id: session_id.clone(),
                            turn_id: turn_id.clone(),
                            part_index: resp_seq,
                            is_web: false,
                            is_response: true,
                        });
                    }
                }
            }
        }
    }

    if allow_web {
        let rows = sqlx::query_as::<_, (String, String, Option<String>, Option<String>)>(
            "SELECT session_id, id, process, response FROM web_record_turns WHERE tenant_id = ?1 AND (process IS NOT NULL OR response IS NOT NULL)",
        )
        .bind(tenant_id)
        .fetch_all(pool)
        .await
        .map_err(StoreError::external)?;

        for (session_id, turn_id, process_json, response_text) in rows {
            let process: Option<TurnProcess> = process_json
                .as_deref()
                .and_then(|s| serde_json::from_str(s).ok());

            if let Some(ref p) = process {
                for item in &p.items {
                    let full_id = derive_standard_part_id("web-record-part", &turn_id, item.seq);
                    if matches_hash_or_key(&full_id, &lower_clean) {
                        candidates.push(TriadPartCandidate {
                            full_id,
                            session_id: session_id.clone(),
                            turn_id: turn_id.clone(),
                            part_index: item.seq,
                            is_web: true,
                            is_response: false,
                        });
                    }
                }
            }

            if let Some(ref resp) = response_text {
                if !resp.trim().is_empty() {
                    let max_seq = process
                        .as_ref()
                        .and_then(|p| p.items.iter().map(|i| i.seq).max())
                        .unwrap_or(-1);
                    let resp_seq = process
                        .as_ref()
                        .and_then(|p| p.response_seq)
                        .unwrap_or(max_seq + 1);
                    let full_id = derive_standard_part_id("web-record-part", &turn_id, resp_seq);
                    if matches_hash_or_key(&full_id, &lower_clean) {
                        candidates.push(TriadPartCandidate {
                            full_id,
                            session_id: session_id.clone(),
                            turn_id: turn_id.clone(),
                            part_index: resp_seq,
                            is_web: true,
                            is_response: true,
                        });
                    }
                }
            }
        }
    }

    Ok(candidates)
}

fn matches_hash_or_key(full_id: &str, clean_prefix: &str) -> bool {
    let clean_id = full_id
        .strip_prefix("conversation-part-")
        .or_else(|| full_id.strip_prefix("web-record-part-"))
        .unwrap_or(full_id);
    if clean_id.starts_with(clean_prefix) || full_id.starts_with(clean_prefix) {
        return true;
    }
    if let Some(id_key) = derive_part_id_key(full_id) {
        let key_hex: String = id_key.iter().map(|b| format!("{:02x}", b)).collect();
        if key_hex.starts_with(clean_prefix) {
            return true;
        }
    }
    false
}

pub async fn load_triad_block_detail(
    pool: &SqlitePool,
    tenant_id: &str,
    record_kind: ConversationRecordKind,
    resolved_part_id: &str,
    block_id: &str,
) -> StoreResult<Option<ConversationBlockDetail>> {
    let is_web = record_kind == ConversationRecordKind::Web;
    let table = if is_web {
        "web_record_turns"
    } else {
        "conversation_turns"
    };

    let rows = sqlx::query_as::<_, (String, String, Option<String>, Option<String>, String)>(
        AssertSqlSafe(format!("SELECT session_id, id, process, response, question_id FROM {table} WHERE tenant_id = ?1 AND (process IS NOT NULL OR response IS NOT NULL)"))
    )
    .bind(tenant_id)
    .fetch_all(pool)
    .await
    .map_err(StoreError::external)?;

    for (session_id, turn_id, process_json, response_text, question_id) in rows {
        let process: Option<TurnProcess> = process_json
            .as_deref()
            .and_then(|s| serde_json::from_str(s).ok());

        // Check process items
        if let Some(ref p) = process {
            for item in &p.items {
                let part_id = if is_web {
                    derive_standard_part_id("web-record-part", &turn_id, item.seq)
                } else {
                    derive_conversation_part_id(&turn_id, item.seq)
                };
                if part_id == resolved_part_id {
                    let (adapter_id, card_kinds) =
                        load_conversation_card_projection_context_for_record_sqlx(
                            pool,
                            tenant_id,
                            record_kind,
                            &session_id,
                        )
                        .await?;

                    let cwd = item.cwd.clone().or_else(|| p.cwd.clone());
                    let source_execution_id = Some(format!("exec-{turn_id}-{}", item.seq));
                    let part = ConversationPart {
                        id: part_id.clone(),
                        turn_id: turn_id.clone(),
                        part_index: item.seq,
                        role: ConversationPartRole::Tool,
                        kind: ConversationPartKind::Command,
                        text: item.summary.clone(),
                        language: None,
                        command: item.content.clone(),
                        cwd: cwd.clone(),
                        status: item.status.clone(),
                        exit_code: item.exit_code,
                        command_label: item.label.clone(),
                        source_execution_id,
                        content_card: Some(ConversationContentCardDescriptor {
                            schema_version: 1,
                            kind: item.kind.clone(),
                            renderer: None,
                            semantic_role: None,
                        }),
                        metadata_json: None,
                        translated_text: item.translated_text.clone(),
                        signal: item.signal.clone(),
                        tool_name: None,
                        file_path: None,
                    };

                    let cards =
                        crate::backend::domain::conversations::projection::project_conversation_content_cards(
                            &part,
                            &adapter_id,
                            &card_kinds,
                        )?;
                    let card = cards
                        .iter()
                        .find(|card| card.node_id == block_id || card.node_id == resolved_part_id)
                        .or_else(|| cards.first());

                    if let Some(card) = card {
                        let mut locator = conversation_card_block_locator(
                            record_kind,
                            &session_id,
                            &question_id,
                            &part,
                            card,
                        );
                        if block_id == part.id
                            || resolved_part_id == part.id
                            || crate::backend::domain::conversation_id_fragment(&part.id)
                                == block_id
                        {
                            locator.block_id = block_id.to_string();
                        }
                        return Ok(Some(ConversationBlockDetail {
                            locator,
                            content: card.body.clone(),
                            translated_content: card.translated_body.clone(),
                        }));
                    } else {
                        let content = item
                            .content
                            .clone()
                            .or_else(|| item.summary.clone())
                            .unwrap_or_default();
                        let locator = fallback_locator(
                            record_kind,
                            &session_id,
                            &question_id,
                            &part,
                            block_id,
                            item.kind.clone(),
                            item.label.clone(),
                            None,
                            content.chars().count(),
                        );
                        return Ok(Some(ConversationBlockDetail {
                            locator,
                            content,
                            translated_content: item.translated_text.clone(),
                        }));
                    }
                }
            }
        }

        // Check response
        if let Some(ref resp) = response_text {
            if !resp.trim().is_empty() {
                let max_seq = process
                    .as_ref()
                    .and_then(|p| p.items.iter().map(|i| i.seq).max())
                    .unwrap_or(-1);
                let resp_seq = process
                    .as_ref()
                    .and_then(|p| p.response_seq)
                    .unwrap_or(max_seq + 1);
                let part_id = if is_web {
                    derive_standard_part_id("web-record-part", &turn_id, resp_seq)
                } else {
                    derive_conversation_part_id(&turn_id, resp_seq)
                };

                if part_id == resolved_part_id {
                    let (adapter_id, card_kinds) =
                        load_conversation_card_projection_context_for_record_sqlx(
                            pool,
                            tenant_id,
                            record_kind,
                            &session_id,
                        )
                        .await?;

                    let resp_card_kind = if is_web {
                        format!("{adapter_id}.answer")
                    } else {
                        process
                            .as_ref()
                            .and_then(|p| p.response_card_kind.clone())
                            .unwrap_or_else(|| "answer".to_string())
                    };

                    let part = ConversationPart {
                        id: part_id.clone(),
                        turn_id: turn_id.clone(),
                        part_index: resp_seq,
                        role: ConversationPartRole::Assistant,
                        kind: ConversationPartKind::Text,
                        text: Some(resp.clone()),
                        language: None,
                        command: None,
                        cwd: None,
                        status: None,
                        exit_code: None,
                        command_label: None,
                        source_execution_id: None,
                        content_card: Some(ConversationContentCardDescriptor {
                            schema_version: 1,
                            kind: resp_card_kind.clone(),
                            renderer: None,
                            semantic_role: None,
                        }),
                        metadata_json: None,
                        translated_text: process
                            .as_ref()
                            .and_then(|p| p.response_translated_text.clone()),
                        signal: None,
                        tool_name: None,
                        file_path: None,
                    };

                    let cards =
                        crate::backend::domain::conversations::projection::project_conversation_content_cards(
                            &part,
                            &adapter_id,
                            &card_kinds,
                        )?;
                    let card = cards
                        .iter()
                        .find(|card| card.node_id == block_id || card.node_id == resolved_part_id)
                        .or_else(|| cards.first());

                    if let Some(card) = card {
                        let mut locator = conversation_card_block_locator(
                            record_kind,
                            &session_id,
                            &question_id,
                            &part,
                            card,
                        );
                        if block_id == part.id
                            || resolved_part_id == part.id
                            || crate::backend::domain::conversation_id_fragment(&part.id)
                                == block_id
                        {
                            locator.block_id = block_id.to_string();
                        }
                        return Ok(Some(ConversationBlockDetail {
                            locator,
                            content: card.body.clone(),
                            translated_content: card.translated_body.clone(),
                        }));
                    } else {
                        let locator = fallback_locator(
                            record_kind,
                            &session_id,
                            &question_id,
                            &part,
                            block_id,
                            resp_card_kind,
                            None,
                            Some("answer".to_string()),
                            resp.chars().count(),
                        );
                        return Ok(Some(ConversationBlockDetail {
                            locator,
                            content: resp.clone(),
                            translated_content: process
                                .as_ref()
                                .and_then(|p| p.response_translated_text.clone()),
                        }));
                    }
                }
            }
        }
    }

    Ok(None)
}

pub use super::triad_update::*;

#[allow(clippy::too_many_arguments)]
fn fallback_locator(
    record_kind: ConversationRecordKind,
    session_id: &str,
    question_id: &str,
    part: &ConversationPart,
    block_id: &str,
    kind: String,
    label: Option<String>,
    semantic_role: Option<String>,
    content_len: usize,
) -> ConversationBlockLocator {
    ConversationBlockLocator {
        record_kind: conversation_record_kind_label(record_kind).to_string(),
        session_id: session_id.to_string(),
        question_id: question_id.to_string(),
        turn_id: part.turn_id.clone(),
        block_id: block_id.to_string(),
        part_id: Some(part.id.clone()),
        kind,
        label,
        semantic_role,
        renderer: ConversationCardRenderer::Plain,
        role: part.role,
        content_length: content_len,
        language: part.language.clone(),
        cwd: part.cwd.clone(),
        status: part.status.clone(),
        exit_code: part.exit_code,
    }
}
