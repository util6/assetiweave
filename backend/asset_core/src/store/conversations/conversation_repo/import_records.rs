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
    #[sqlx(default)]
    last_cwd: Option<String>,
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
            last_cwd: self.last_cwd,
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
            last_cwd: self.cwd.clone(),
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

/// 会话是否未发生变化。
///
/// 版本凭据是**不透明**的：只比较"本轮发现到的业务版本令牌"与"上次成功导入时落库的令牌"，
/// 相等即未变化、不等即需要整包重扫。这里刻意不看标题、路径、起止时间等展示属性，
/// 也不做全局单调时间戳比较——只要宿主业务记录没给出新令牌，就说明会话内容没有变化。
///
/// 没有落库令牌（首次导入、或上次失败未推进水线）一律视为已变化，保证失败可重试。
pub(super) async fn conversation_session_is_unchanged_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    session: &ConversationSession,
) -> StoreResult<bool> {
    let Some(discovered) = session.source_fingerprint.as_deref() else {
        return Ok(false);
    };
    let stored: Option<Option<String>> = sqlx::query_scalar(
        r#"
        SELECT source_fingerprint
        FROM conversation_sessions
        WHERE tenant_id = ?1 AND source_id = ?2 AND external_id = ?3
        "#,
    )
    .bind(tenant_id)
    .bind(&session.source_id)
    .bind(&session.external_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(StoreError::external)?;

    Ok(stored.flatten().as_deref() == Some(discovered))
}

pub(crate) async fn upsert_conversation_session_sqlx_tx(
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
