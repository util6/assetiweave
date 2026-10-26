use super::*;

/// 零件级全文索引的状态取值。`ready` 之外的状态意味着正文匹配不可用，列表搜索降级为标题级。
pub(crate) const PART_INDEX_READY: &str = "ready";
pub(crate) const PART_INDEX_BUILDING: &str = "building";
pub(crate) const PART_INDEX_STALE: &str = "stale";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConversationPartIndexState {
    pub tenant_id: String,
    pub status: String,
    pub part_count: i64,
    pub last_error: Option<String>,
    pub rebuilt_at: Option<String>,
    pub updated_at: Option<String>,
}

impl ConversationPartIndexState {
    pub fn is_ready(&self) -> bool {
        self.status == PART_INDEX_READY
    }
}

#[derive(Debug, FromRow)]
struct PartIndexStateRow {
    tenant_id: String,
    status: String,
    part_count: i64,
    last_error: Option<String>,
    rebuilt_at: Option<String>,
    updated_at: Option<String>,
}

pub async fn load_conversation_part_index_state_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
) -> StoreResult<Option<ConversationPartIndexState>> {
    let row = sqlx::query_as::<_, PartIndexStateRow>(
        r#"
        SELECT tenant_id, status, part_count, last_error, rebuilt_at, updated_at
        FROM conversation_part_index_state
        WHERE tenant_id = ?1
        "#,
    )
    .bind(tenant_id)
    .fetch_optional(pool)
    .await
    .map_err(StoreError::external)?;
    Ok(row.map(|row| ConversationPartIndexState {
        tenant_id: row.tenant_id,
        status: row.status,
        part_count: row.part_count,
        last_error: row.last_error,
        rebuilt_at: row.rebuilt_at,
        updated_at: row.updated_at,
    }))
}

/// 索引是否可用于正文匹配。缺行即视为就绪：没有任何写入的租户不需要初始化。
pub async fn conversation_part_index_ready_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
) -> StoreResult<bool> {
    Ok(load_conversation_part_index_state_sqlx(pool, tenant_id)
        .await?
        .map(|state| state.is_ready())
        .unwrap_or(true))
}

pub async fn set_conversation_part_index_status_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    status: &str,
    part_count: i64,
    last_error: Option<&str>,
) -> StoreResult<()> {
    let now = Utc::now().to_rfc3339();
    let rebuilt_at = (status == PART_INDEX_READY).then(|| now.clone());
    sqlx::query(
        r#"
        INSERT INTO conversation_part_index_state (
            tenant_id, status, part_count, last_error, rebuilt_at, updated_at
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6)
        ON CONFLICT(tenant_id) DO UPDATE SET
            status = excluded.status,
            part_count = excluded.part_count,
            last_error = excluded.last_error,
            rebuilt_at = COALESCE(excluded.rebuilt_at, conversation_part_index_state.rebuilt_at),
            updated_at = excluded.updated_at
        "#,
    )
    .bind(tenant_id)
    .bind(status)
    .bind(part_count)
    .bind(last_error)
    .bind(rebuilt_at)
    .bind(now)
    .execute(pool)
    .await
    .map_err(StoreError::external)?;
    Ok(())
}
