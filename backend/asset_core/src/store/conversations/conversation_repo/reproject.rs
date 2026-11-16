use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use super::*;
use crate::backend::domain::conversations::triad::{split_normalized_turn, TriadPart, TurnProcess};
use crate::backend::domain::conversations::NormalizedConversationPart;
use crate::backend::store::conversations::snapshot::{
    fold_part_attrs_paths, fold_turn_process_paths, load_existing_session_snapshot_sqlx_tx,
    prepare_session_snapshot, replace_session_snapshot_sqlx_tx, restore_turn_parts,
    ExistingTurnSnapshot, PreparedTurn,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReprojectStoredTurnPayload {
    pub id: String,
    pub turn_index: i64,
    pub prompt: String,
    pub response: Option<String>,
    pub parts: Vec<ConversationPart>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReprojectStoredSessionPayload {
    pub id: String,
    pub adapter_id: String,
    pub workspace_path: Option<String>,
    pub turns: Vec<ReprojectStoredTurnPayload>,
}

#[derive(Debug, Clone)]
pub struct StoredSessionReprojectionOrigin {
    pub session: ConversationSession,
    pub existing_turns: Vec<ExistingTurnSnapshot>,
    pub payload: ReprojectStoredSessionPayload,
}

#[derive(Debug, Clone, Default)]
pub struct LegacyReprojectionReport {
    pub scanned_sessions: usize,
    pub updated_sessions: usize,
    pub skipped_unchanged_sessions: usize,
    pub failed_sessions: usize,
}

pub async fn load_sessions_for_reprojection_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    source_id: Option<&str>,
    adapter_id: Option<&str>,
    exclude_session_ids: &BTreeSet<String>,
) -> StoreResult<Vec<StoredSessionReprojectionOrigin>> {
    let rows = sqlx::query(
        r#"
        SELECT id, source_id, adapter_id, external_id, title, project_path,
               started_at, updated_at, source_locator, source_fingerprint,
               missing, created_at, imported_at, execution_origin,
               execution_purpose, user_visible, last_cwd
        FROM conversation_sessions
        WHERE tenant_id = ?1
          AND (?2 IS NULL OR source_id = ?2)
          AND (?3 IS NULL OR adapter_id = ?3)
        ORDER BY updated_at DESC, created_at DESC
        "#,
    )
    .bind(tenant_id)
    .bind(source_id)
    .bind(adapter_id)
    .fetch_all(pool)
    .await
    .map_err(StoreError::external)?;
    let sessions: Vec<ConversationSession> = rows
        .iter()
        .map(map_sqlx_conversation_session)
        .collect::<StoreResult<Vec<_>>>()?;

    let mut tx = pool.begin().await.map_err(StoreError::external)?;
    let tables = ConversationRecordKind::Session.record_tables();
    let mut loaded = Vec::new();
    for session in sessions {
        if exclude_session_ids.contains(&session.id) {
            continue;
        }
        let (_, existing_turns) = load_existing_session_snapshot_sqlx_tx(
            &mut tx,
            tenant_id,
            ConversationRecordKind::Session,
            &session.id,
        )
        .await?;
        if existing_turns.is_empty() {
            continue;
        }
        let payload_turns = existing_turns
            .iter()
            .map(|existing| {
                let parts = restore_turn_parts(
                    &session.adapter_id,
                    tables.part_id_prefix,
                    &existing.turn.id,
                    &existing.turn.prompt,
                    existing.turn.process.as_deref(),
                    existing.turn.response.as_deref(),
                    &existing.parts,
                );
                ReprojectStoredTurnPayload {
                    id: existing.turn.id.clone(),
                    turn_index: existing.turn.turn_index,
                    prompt: existing.turn.prompt.clone(),
                    response: existing.turn.response.clone(),
                    parts,
                }
            })
            .collect();
        let payload = ReprojectStoredSessionPayload {
            id: session.id.clone(),
            adapter_id: session.adapter_id.clone(),
            workspace_path: session
                .last_cwd
                .clone()
                .or_else(|| session.project_path.clone()),
            turns: payload_turns,
        };
        loaded.push(StoredSessionReprojectionOrigin {
            session,
            existing_turns,
            payload,
        });
    }
    tx.commit().await.map_err(StoreError::external)?;
    Ok(loaded)
}

fn normalized_part_from_stored(part: &ConversationPart) -> NormalizedConversationPart {
    NormalizedConversationPart {
        role: part.role,
        kind: part.kind,
        text: part.text.clone(),
        language: part.language.clone(),
        command: part.command.clone(),
        cwd: part.cwd.clone(),
        status: part.status.clone(),
        exit_code: part.exit_code,
        command_label: part.command_label.clone(),
        source_execution_id: part.source_execution_id.clone(),
        content_card: part.content_card.clone(),
        metadata_json: part.metadata_json.clone(),
    }
}

fn migrate_part_translations(old_parts: &[TriadPart], new_parts: &mut [TriadPart]) {
    for part in new_parts.iter_mut() {
        if part.translated_text.is_some() {
            continue;
        }
        if let Some(matched) = old_parts.iter().find(|op| {
            op.translated_text.is_some() && op.kind == part.kind && op.content == part.content
        }) {
            part.translated_text = matched.translated_text.clone();
        } else if part
            .content
            .as_deref()
            .is_some_and(|c| !c.trim().is_empty())
        {
            if let Some(matched) = old_parts
                .iter()
                .find(|op| op.translated_text.is_some() && op.content == part.content)
            {
                part.translated_text = matched.translated_text.clone();
            }
        }
    }
}

fn build_reprojected_prepared_turns(
    origin: &StoredSessionReprojectionOrigin,
    reprojected: &ReprojectStoredSessionPayload,
) -> StoreResult<Option<Vec<PreparedTurn>>> {
    if reprojected.turns.len() != origin.existing_turns.len() {
        return Err(StoreError::Validation(format!(
            "reprojection invariant violated for session {}: expected {} turns, got {}",
            origin.session.id,
            origin.existing_turns.len(),
            reprojected.turns.len()
        )));
    }

    let mut prepared = Vec::with_capacity(origin.existing_turns.len());
    let mut any_changed = false;

    for (orig_turn, next_turn) in origin.existing_turns.iter().zip(reprojected.turns.iter()) {
        if next_turn.id != orig_turn.turn.id
            || next_turn.prompt != orig_turn.turn.prompt
            || next_turn.response != orig_turn.turn.response
        {
            return Err(StoreError::Validation(format!(
                "reprojection QA invariant violated for session {} turn {}",
                origin.session.id, orig_turn.turn.id
            )));
        }

        let norm_parts: Vec<NormalizedConversationPart> = next_turn
            .parts
            .iter()
            .map(normalized_part_from_stored)
            .collect();
        let triad = split_normalized_turn(
            &origin.session.adapter_id,
            &orig_turn.turn.prompt,
            &norm_parts,
        );

        let mut merged_process = orig_turn
            .turn
            .process
            .as_deref()
            .and_then(|json| serde_json::from_str::<TurnProcess>(json).ok())
            .unwrap_or_default();
        let old_items = merged_process.items.clone();
        let mut next_items = triad
            .process
            .map(|proc_val| proc_val.items)
            .unwrap_or_default();
        for item in &mut next_items {
            if item.translated_text.is_some() {
                continue;
            }
            if let Some(matched) = old_items
                .iter()
                .find(|oi| oi.translated_text.is_some() && oi.content == item.content)
            {
                item.translated_text = matched.translated_text.clone();
            }
        }
        merged_process.items = next_items;
        fold_turn_process_paths(&mut merged_process);
        let next_process_json = (!merged_process.is_empty())
            .then(|| serde_json::to_string(&merged_process).unwrap_or_default());

        let mut next_parts = fold_part_attrs_paths(triad.parts);
        migrate_part_translations(&orig_turn.parts, &mut next_parts);

        if next_process_json != orig_turn.turn.process || next_parts != orig_turn.parts {
            any_changed = true;
        }

        // Structurally preserve every Turn column from DB except `process` items.
        let mut preserved_turn = orig_turn.turn.clone();
        preserved_turn.process = next_process_json;

        prepared.push(PreparedTurn {
            turn: preserved_turn,
            parts: next_parts,
        });
    }

    Ok(any_changed.then_some(prepared))
}

pub async fn apply_reprojected_sessions_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    origins: &[StoredSessionReprojectionOrigin],
    reprojected: &[ReprojectStoredSessionPayload],
) -> StoreResult<LegacyReprojectionReport> {
    let by_id: BTreeMap<&str, &ReprojectStoredSessionPayload> =
        reprojected.iter().map(|s| (s.id.as_str(), s)).collect();
    let mut report = LegacyReprojectionReport {
        scanned_sessions: origins.len(),
        ..Default::default()
    };
    let mut changed_by_source: BTreeMap<String, Vec<String>> = BTreeMap::new();

    for origin in origins {
        let Some(next) = by_id.get(origin.session.id.as_str()) else {
            report.failed_sessions += 1;
            continue;
        };
        let prepared = match build_reprojected_prepared_turns(origin, next) {
            Ok(Some(prepared)) => prepared,
            Ok(None) => {
                report.skipped_unchanged_sessions += 1;
                continue;
            }
            Err(err) => {
                tracing::warn!(
                    session_id = %origin.session.id,
                    error = %err,
                    "skipping legacy session reprojection due to QA invariant check"
                );
                report.failed_sessions += 1;
                continue;
            }
        };

        let mut tx = begin_immediate_with_retry(pool, 8).await?;
        replace_session_snapshot_sqlx_tx(
            &mut tx,
            tenant_id,
            ConversationRecordKind::Session,
            &origin.session,
            &prepared,
        )
        .await?;
        tx.commit().await.map_err(StoreError::external)?;
        report.updated_sessions += 1;
        changed_by_source
            .entry(origin.session.source_id.clone())
            .or_default()
            .push(origin.session.id.clone());
    }

    if !changed_by_source.is_empty() {
        let now = Utc::now().to_rfc3339();
        let mut final_tx = begin_immediate_with_retry(pool, 8).await?;
        let revision =
            super::bump_conversation_search_source_revision_sqlx_tx(&mut *final_tx, tenant_id)
                .await?;
        for (source_id, session_ids) in changed_by_source {
            let sync_run_id = stable_id("conversation-reproject", &[&source_id, &now]);
            crate::backend::store::append_outbox_event_sqlx_tx(
                &mut final_tx,
                &DomainEvent::conversation_source_committed(
                    tenant_id,
                    &sync_run_id,
                    &source_id,
                    revision,
                    session_ids,
                ),
            )
            .await?;
        }
        final_tx.commit().await.map_err(StoreError::external)?;
    }

    Ok(report)
}

pub async fn reproject_conversation_session_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    source: &ConversationSource,
    normalized: &NormalizedConversationSession,
    adapter_content_hash: Option<&str>,
    card_contract_version: Option<u32>,
    payload_policy_version: u32,
    projection_version: Option<u32>,
) -> StoreResult<()> {
    let now = Utc::now().to_rfc3339();
    let mut tx = pool.begin().await.map_err(StoreError::external)?;

    let session = conversation_session_from_normalized(source, normalized, &now);
    let prepared = prepare_session_snapshot(
        ConversationRecordKind::Session,
        &source.id,
        &session.id,
        &session.adapter_id,
        normalized.execution_origin.as_deref(),
        session.started_at.as_deref(),
        session.updated_at.as_deref(),
        &normalized.turns,
        &now,
    );

    upsert_conversation_session_sqlx_tx(&mut tx, tenant_id, &session).await?;
    replace_session_snapshot_sqlx_tx(
        &mut tx,
        tenant_id,
        ConversationRecordKind::Session,
        &session,
        &prepared,
    )
    .await?;

    let last_cwd = derive_session_last_cwd(&normalized.turns);
    update_conversation_session_last_cwd_sqlx_tx(
        &mut tx,
        tenant_id,
        &session.id,
        last_cwd.as_deref(),
    )
    .await?;

    stamp_source_sync_contract_sqlx_tx(
        &mut tx,
        tenant_id,
        &source.id,
        adapter_content_hash,
        card_contract_version,
        payload_policy_version,
        projection_version,
    )
    .await?;

    tx.commit().await.map_err(StoreError::external)?;
    Ok(())
}

#[cfg(test)]
#[path = "reproject_tests.rs"]
mod tests;
