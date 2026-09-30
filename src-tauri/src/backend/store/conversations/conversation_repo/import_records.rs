use super::*;

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
        Ok(ConversationSessionListItem {
            session,
            question_count,
            turn_count,
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
    parts: &[crate::backend::domain::NormalizedConversationPart],
) -> StoreResult<()> {
    let existing_ids = sqlx::query_scalar::<_, String>(
        "SELECT id FROM conversation_parts WHERE tenant_id = ?1 AND turn_id = ?2",
    )
    .bind(tenant_id)
    .bind(turn_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(StoreError::external)?;
    let mut incoming_ids = BTreeSet::new();
    for (index, part) in parts.iter().enumerate() {
        let part_id = stable_id("conversation-part", &[turn_id, &index.to_string()]);
        incoming_ids.insert(part_id.clone());
        let content_card_json = part.content_card.as_ref().map(encode_json).transpose()?;
        sqlx::query(
            r#"
            INSERT INTO conversation_parts (
                tenant_id, id, turn_id, part_index, role, kind, text, language, command,
                cwd, status, exit_code, command_label, metadata_json, content_card_json, source_execution_id
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16)
            ON CONFLICT(tenant_id, id) DO UPDATE SET
                turn_id = excluded.turn_id,
                part_index = excluded.part_index,
                role = excluded.role,
                kind = excluded.kind,
                text = excluded.text,
                language = excluded.language,
                command = excluded.command,
                cwd = excluded.cwd,
                status = excluded.status,
                exit_code = excluded.exit_code,
                command_label = excluded.command_label,
                metadata_json = excluded.metadata_json,
                content_card_json = excluded.content_card_json,
                source_execution_id = excluded.source_execution_id,
                translated_text = CASE
                    WHEN COALESCE(conversation_parts.text, '') = COALESCE(excluded.text, '')
                     AND COALESCE(conversation_parts.command, '') = COALESCE(excluded.command, '')
                    THEN conversation_parts.translated_text
                    ELSE NULL
                END
            "#,
        )
        .bind(tenant_id)
        .bind(part_id)
        .bind(turn_id)
        .bind(index as i64)
        .bind(encode_enum(part.role)?)
        .bind(encode_enum(part.kind)?)
        .bind(&part.text)
        .bind(&part.language)
        .bind(&part.command)
        .bind(&part.cwd)
        .bind(&part.status)
        .bind(part.exit_code)
        .bind(&part.command_label)
        .bind(&part.metadata_json)
        .bind(content_card_json)
        .bind(&part.source_execution_id)
        .execute(&mut **tx)
        .await
        .map_err(StoreError::external)?;
    }
    for stale_id in existing_ids
        .into_iter()
        .filter(|id| !incoming_ids.contains(id))
    {
        sqlx::query("DELETE FROM conversation_parts WHERE tenant_id = ?1 AND id = ?2")
            .bind(tenant_id)
            .bind(stale_id)
            .execute(&mut **tx)
            .await
            .map_err(StoreError::external)?;
    }
    Ok(())
}
