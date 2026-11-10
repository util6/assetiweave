use super::*;
use crate::backend::domain::conversations::search_corpus::search_corpus_policy_version;

/// 零件级全文索引的状态取值。`ready` 之外的状态意味着正文匹配不可用，列表搜索降级为标题级。
pub(crate) const PART_INDEX_READY: &str = "ready";
pub(crate) const PART_INDEX_BUILDING: &str = "building";
pub(crate) const PART_INDEX_STALE: &str = "stale";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConversationPartIndexState {
    pub tenant_id: String,
    pub status: String,
    pub part_count: i64,
    /// 该代索引落盘时使用的语料策略版本。
    pub corpus_policy_version: u32,
    pub last_error: Option<String>,
    pub rebuilt_at: Option<String>,
    pub updated_at: Option<String>,
}

impl ConversationPartIndexState {
    /// 只有状态就绪、且语料口径与本代策略一致时，索引才代表当前事实。
    ///
    /// 策略版本落后时索引仍然可读，但会给出按旧口径生成的命中；此时把正文匹配整体降级为
    /// 标题级，比让用户看到一批不再符合规则的结果更诚实。
    pub fn is_ready(&self) -> bool {
        self.status == PART_INDEX_READY
            && self.corpus_policy_version == search_corpus_policy_version()
    }
}

#[derive(Debug, FromRow)]
struct PartIndexStateRow {
    tenant_id: String,
    status: String,
    part_count: i64,
    corpus_policy_version: i64,
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
        SELECT tenant_id, status, part_count, corpus_policy_version, last_error, rebuilt_at, updated_at
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
        corpus_policy_version: u32::try_from(row.corpus_policy_version).unwrap_or(0),
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
    // 只有"就绪"才代表这一代索引已经用当前语料口径重建完毕；building/stale 保留旧版本号，
    // 使兼容性判定持续为假，直到真正的重建收尾。
    let corpus_policy_version = if status == PART_INDEX_READY {
        search_corpus_policy_version()
    } else {
        0
    };
    sqlx::query(
        r#"
        INSERT INTO conversation_part_index_state (
            tenant_id, status, part_count, corpus_policy_version, last_error, rebuilt_at, updated_at
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
        ON CONFLICT(tenant_id) DO UPDATE SET
            status = excluded.status,
            part_count = excluded.part_count,
            corpus_policy_version = CASE
                WHEN excluded.status = 'ready' THEN excluded.corpus_policy_version
                ELSE conversation_part_index_state.corpus_policy_version
            END,
            last_error = excluded.last_error,
            rebuilt_at = COALESCE(excluded.rebuilt_at, conversation_part_index_state.rebuilt_at),
            updated_at = excluded.updated_at
        "#,
    )
    .bind(tenant_id)
    .bind(status)
    .bind(part_count)
    .bind(i64::from(corpus_policy_version))
    .bind(last_error)
    .bind(rebuilt_at)
    .bind(now)
    .execute(pool)
    .await
    .map_err(StoreError::external)?;
    Ok(())
}
