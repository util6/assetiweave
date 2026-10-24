use super::*;
use crate::domain::conversations::{
    derive_conversation_part_id, derive_part_id_key, ConversationPartKind,
    NormalizedConversationPart,
};

#[derive(Debug, FromRow)]
pub(super) struct ConversationSessionListItemRow {
    id: String,
    source_id: String,
    adapter_id: String,
    external_id: String,
    title: String,
    project_path: Option<String>,
    started_at: Option<String>,
    updated_at: Option<String>,
    source_locator: Option<String>,
    source_fingerprint: Option<String>,
    missing: i64,
    created_at: String,
    imported_at: String,
    question_count: i64,
    turn_count: i64,
    #[sqlx(default)]
    latest_user_prompt: Option<String>,
    #[sqlx(default)]
    window_turn_count: Option<i64>,
    #[sqlx(default)]
    window_start_turn_index: Option<i64>,
    #[sqlx(default)]
    window_start_user_prompt: Option<String>,
    #[sqlx(default)]
    window_end_turn_index: Option<i64>,
    #[sqlx(default)]
    window_end_user_prompt: Option<String>,
    #[sqlx(default)]
    execution_origin: Option<String>,
    #[sqlx(default)]
    execution_purpose: Option<String>,
    #[sqlx(default)]
    user_visible: Option<i64>,
}

impl ConversationSessionListItemRow {
    pub(super) fn into_item(self) -> StoreResult<ConversationSessionListItem> {
        let question_count = usize::try_from(self.question_count)
            .map_err(|_| StoreError::external("invalid conversation question count"))?;
        let turn_count = usize::try_from(self.turn_count)
            .map_err(|_| StoreError::external("invalid conversation turn count"))?;
        let session = ConversationSession {
            id: self.id,
            source_id: self.source_id,
            adapter_id: self.adapter_id,
            external_id: self.external_id,
            title: self.title,
            project_path: self.project_path,
            started_at: self.started_at,
            updated_at: self.updated_at,
            source_locator: self.source_locator,
            source_fingerprint: self.source_fingerprint,
            missing: self.missing == 1,
            created_at: self.created_at,
            imported_at: self.imported_at,
            execution_origin: self.execution_origin.unwrap_or_else(|| "user".to_string()),
            execution_purpose: self.execution_purpose,
            user_visible: self.user_visible.map(|v| v != 0).unwrap_or(true),
        };
        let window_turn_count = self.window_turn_count.and_then(|c| usize::try_from(c).ok());
        let window_start_turn_index = self
            .window_start_turn_index
            .and_then(|c| usize::try_from(c).ok());
        let window_end_turn_index = self
            .window_end_turn_index
            .and_then(|c| usize::try_from(c).ok());
        Ok(ConversationSessionListItem {
            session,
            question_count,
            turn_count,
            latest_user_prompt: self.latest_user_prompt,
            window_turn_count,
            window_start_turn_index,
            window_start_user_prompt: self.window_start_user_prompt,
            window_end_turn_index,
            window_end_user_prompt: self.window_end_user_prompt,
        })
    }
}

#[derive(Debug, FromRow)]
pub(super) struct RecentConversationSessionRecordRow {
    id: String,
    source_id: String,
    adapter_id: String,
    external_id: String,
    title: String,
    project_path: Option<String>,
    started_at: Option<String>,
    updated_at: Option<String>,
    source_locator: Option<String>,
    source_fingerprint: Option<String>,
    missing: i64,
    created_at: String,
    imported_at: String,
    question_count: i64,
    turn_count: i64,
    last_activity_at: String,
    cwd: Option<String>,
    source_agent: String,
    #[sqlx(default)]
    execution_origin: Option<String>,
    #[sqlx(default)]
    execution_purpose: Option<String>,
    #[sqlx(default)]
    user_visible: Option<i64>,
}

impl RecentConversationSessionRecordRow {
    pub(super) fn into_record(self) -> StoreResult<RecentConversationSessionRecord> {
        let question_count = usize::try_from(self.question_count)
            .map_err(|_| StoreError::external("invalid recent question count"))?;
        let turn_count = usize::try_from(self.turn_count)
            .map_err(|_| StoreError::external("invalid recent turn count"))?;
        let session = ConversationSession {
            id: self.id,
            source_id: self.source_id,
            adapter_id: self.adapter_id,
            external_id: self.external_id,
            title: self.title,
            project_path: self.project_path,
            started_at: self.started_at,
            updated_at: self.updated_at,
            source_locator: self.source_locator,
            source_fingerprint: self.source_fingerprint,
            missing: self.missing == 1,
            created_at: self.created_at,
            imported_at: self.imported_at,
            execution_origin: self.execution_origin.unwrap_or_else(|| "user".to_string()),
            execution_purpose: self.execution_purpose,
            user_visible: self.user_visible.map(|v| v != 0).unwrap_or(true),
        };
        Ok(RecentConversationSessionRecord {
            session: ConversationSessionListItem {
                session,
                question_count,
                turn_count,
                latest_user_prompt: None,
                window_turn_count: None,
                window_start_turn_index: None,
                window_start_user_prompt: None,
                window_end_turn_index: None,
                window_end_user_prompt: None,
            },
            last_activity_at: self.last_activity_at,
            cwd: self.cwd,
            source_agent: self.source_agent,
            recent_events: Vec::new(),
        })
    }
}

pub(super) async fn conversation_session_is_unchanged_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    session: &ConversationSession,
    normalized: &NormalizedConversationSession,
) -> StoreResult<bool> {
    let Some(source_fingerprint) = session.source_fingerprint.as_deref() else {
        return Ok(false);
    };
    #[derive(Debug, FromRow)]
    struct ConversationSessionUnchangedCheckRow {
        title: String,
        project_path: Option<String>,
        started_at: Option<String>,
        updated_at: Option<String>,
        source_locator: Option<String>,
        source_fingerprint: Option<String>,
        missing: i64,
    }

    let Some(row) = sqlx::query_as::<_, ConversationSessionUnchangedCheckRow>(
        r#"
        SELECT title, project_path, started_at, updated_at, source_locator,
               source_fingerprint, missing
        FROM conversation_sessions
        WHERE tenant_id = ?1 AND source_id = ?2 AND external_id = ?3
        "#,
    )
    .bind(tenant_id)
    .bind(&session.source_id)
    .bind(&session.external_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(StoreError::external)?
    else {
        return Ok(false);
    };

    Ok(row.title == session.title
        && row.project_path == session.project_path
        && row.started_at == session.started_at
        && row.updated_at == session.updated_at
        && row.source_locator == session.source_locator
        && row.source_fingerprint.as_deref() == Some(source_fingerprint)
        && row.missing == 0
        && conversation_session_turns_are_unchanged_sqlx_tx(tx, tenant_id, &session.id, normalized)
            .await?)
}

pub(super) async fn conversation_session_exists_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    session_id: &str,
) -> StoreResult<bool> {
    let exists = sqlx::query_scalar::<_, i64>(
        "SELECT EXISTS(SELECT 1 FROM conversation_sessions WHERE tenant_id = ?1 AND id = ?2)",
    )
    .bind(tenant_id)
    .bind(session_id)
    .fetch_one(&mut **tx)
    .await
    .map_err(StoreError::external)?;
    Ok(exists != 0)
}

pub(super) async fn conversation_session_turns_are_unchanged_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    session_id: &str,
    normalized: &NormalizedConversationSession,
) -> StoreResult<bool> {
    #[derive(Debug, FromRow)]
    struct ConversationTurnUnchangedCheckRow {
        external_id: String,
        fingerprint: String,
        missing: i64,
    }

    let rows = sqlx::query_as::<_, ConversationTurnUnchangedCheckRow>(
        r#"
        SELECT external_id, fingerprint, missing
        FROM conversation_turns
        WHERE tenant_id = ?1 AND session_id = ?2
        ORDER BY turn_index ASC, external_id ASC
        "#,
    )
    .bind(tenant_id)
    .bind(session_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(StoreError::external)?;
    if rows.len() != normalized.turns.len() {
        return Ok(false);
    }
    for (row, turn) in rows.iter().zip(&normalized.turns) {
        if row.external_id != turn.external_id
            || row.fingerprint != conversation_turn_fingerprint(turn)
            || row.missing != 0
        {
            return Ok(false);
        }
    }
    Ok(true)
}

pub(super) async fn upsert_conversation_session_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    session: &ConversationSession,
) -> StoreResult<()> {
    sqlx::query(
        r#"
        INSERT INTO conversation_sessions (
            tenant_id, id, source_id, adapter_id, external_id, title, project_path, started_at,
            updated_at, source_locator, source_fingerprint, missing, created_at, imported_at,
            execution_origin, execution_purpose, user_visible
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17)
        ON CONFLICT(tenant_id, source_id, external_id) DO UPDATE SET
            adapter_id = excluded.adapter_id,
            title = excluded.title,
            project_path = excluded.project_path,
            started_at = excluded.started_at,
            updated_at = excluded.updated_at,
            source_locator = excluded.source_locator,
            source_fingerprint = excluded.source_fingerprint,
            missing = 0,
            imported_at = excluded.imported_at,
            execution_origin = excluded.execution_origin,
            execution_purpose = excluded.execution_purpose,
            user_visible = excluded.user_visible
        "#,
    )
    .bind(tenant_id)
    .bind(&session.id)
    .bind(&session.source_id)
    .bind(&session.adapter_id)
    .bind(&session.external_id)
    .bind(&session.title)
    .bind(&session.project_path)
    .bind(&session.started_at)
    .bind(&session.updated_at)
    .bind(&session.source_locator)
    .bind(&session.source_fingerprint)
    .bind(if session.missing { 1_i64 } else { 0_i64 })
    .bind(&session.created_at)
    .bind(&session.imported_at)
    .bind(&session.execution_origin)
    .bind(&session.execution_purpose)
    .bind(if session.user_visible { 1_i64 } else { 0_i64 })
    .execute(&mut **tx)
    .await
    .map_err(StoreError::external)?;
    Ok(())
}

pub(super) async fn upsert_conversation_turn_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    turn: &ConversationTurn,
) -> StoreResult<()> {
    sqlx::query(
        r#"
        INSERT INTO conversation_turns (
            tenant_id, id, session_id, external_id, turn_index, user_text, title, started_at,
            ended_at, fingerprint, missing, imported_at, model
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
        ON CONFLICT(tenant_id, session_id, external_id) DO UPDATE SET
            turn_index = excluded.turn_index,
            user_text = excluded.user_text,
            title = excluded.title,
            started_at = excluded.started_at,
            ended_at = excluded.ended_at,
            fingerprint = excluded.fingerprint,
            missing = 0,
            imported_at = excluded.imported_at,
            model = excluded.model
        "#,
    )
    .bind(tenant_id)
    .bind(&turn.id)
    .bind(&turn.session_id)
    .bind(&turn.external_id)
    .bind(turn.turn_index)
    .bind(&turn.user_text)
    .bind(&turn.title)
    .bind(&turn.started_at)
    .bind(&turn.ended_at)
    .bind(&turn.fingerprint)
    .bind(if turn.missing { 1_i64 } else { 0_i64 })
    .bind(&turn.imported_at)
    .bind(&turn.model)
    .execute(&mut **tx)
    .await
    .map_err(StoreError::external)?;
    Ok(())
}

pub(super) async fn replace_conversation_parts_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    turn_id: &str,
    parts: &[NormalizedConversationPart],
) -> StoreResult<()> {
    for (index, part) in parts.iter().enumerate() {
        let part_index = index as i64;
        let part_id = derive_conversation_part_id(turn_id, part_index);
        let id_key = derive_part_id_key(&part_id).unwrap_or([0u8; 8]);
        let card_kind = part.content_card.as_ref().map(|c| c.kind.clone());

        let (content, summary) =
            if part.kind == ConversationPartKind::Command && part.command.is_some() {
                (part.command.clone(), part.text.clone())
            } else {
                (part.text.clone(), None)
            };

        let mut attrs_map = serde_json::Map::new();
        if let Some(ref val) = part.language {
            if !val.is_empty() {
                attrs_map.insert(
                    "language".to_string(),
                    serde_json::Value::String(val.clone()),
                );
            }
        }
        if let Some(ref val) = part.cwd {
            if !val.is_empty() {
                attrs_map.insert("cwd".to_string(), serde_json::Value::String(val.clone()));
            }
        }
        if let Some(ref val) = part.status {
            if !val.is_empty() {
                attrs_map.insert("status".to_string(), serde_json::Value::String(val.clone()));
            }
        }
        if let Some(val) = part.exit_code {
            attrs_map.insert(
                "exit_code".to_string(),
                serde_json::Value::Number(val.into()),
            );
        }
        if let Some(ref val) = part.command_label {
            if !val.is_empty() {
                attrs_map.insert(
                    "command_label".to_string(),
                    serde_json::Value::String(val.clone()),
                );
            }
        }
        if let Some(ref val) = part.source_execution_id {
            if !val.is_empty() {
                attrs_map.insert(
                    "exec_id".to_string(),
                    serde_json::Value::String(val.clone()),
                );
            }
        }
        if let Some(ref val) = summary {
            if !val.trim().is_empty() {
                attrs_map.insert(
                    "summary".to_string(),
                    serde_json::Value::String(val.clone()),
                );
            }
        }
        if let Some(ref card) = part.content_card {
            if let Some(ref renderer) = card.renderer {
                attrs_map.insert(
                    "renderer".to_string(),
                    serde_json::Value::String(renderer.clone()),
                );
            }
            if let Some(ref semantic_role) = card.semantic_role {
                attrs_map.insert(
                    "semantic_role".to_string(),
                    serde_json::Value::String(semantic_role.clone()),
                );
            }
        }

        let attrs_json = if attrs_map.is_empty() {
            None
        } else {
            Some(
                serde_json::to_string(&serde_json::Value::Object(attrs_map))
                    .map_err(StoreError::external)?,
            )
        };

        sqlx::query(
            r#"
            INSERT INTO conversation_parts (
                tenant_id, turn_id, part_index, id_key, role, kind, card_kind,
                content, attrs_json, metadata_json
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
            ON CONFLICT(tenant_id, turn_id, part_index) DO UPDATE SET
                id_key = excluded.id_key,
                role = excluded.role,
                kind = excluded.kind,
                card_kind = excluded.card_kind,
                content = excluded.content,
                attrs_json = excluded.attrs_json,
                metadata_json = excluded.metadata_json,
                translated_text = CASE
                    WHEN COALESCE(conversation_parts.content, '') = COALESCE(excluded.content, '')
                    THEN conversation_parts.translated_text
                    ELSE NULL
                END
            "#,
        )
        .bind(tenant_id)
        .bind(turn_id)
        .bind(part_index)
        .bind(id_key.as_slice())
        .bind(encode_enum(part.role)?)
        .bind(encode_enum(part.kind)?)
        .bind(card_kind)
        .bind(content)
        .bind(attrs_json)
        .bind(&part.metadata_json)
        .execute(&mut **tx)
        .await
        .map_err(StoreError::external)?;
    }

    // 清理尾部多余零件
    sqlx::query(
        "DELETE FROM conversation_parts WHERE tenant_id = ?1 AND turn_id = ?2 AND part_index >= ?3",
    )
    .bind(tenant_id)
    .bind(turn_id)
    .bind(parts.len() as i64)
    .execute(&mut **tx)
    .await
    .map_err(StoreError::external)?;

    // 索引随零件写入增量维护：只重建这个轮次的索引行，不触碰其它轮次。
    replace_turn_part_index_sqlx_tx(tx, tenant_id, turn_id, ConversationRecordKind::Session)
        .await?;
    Ok(())
}
