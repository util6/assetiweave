//! 会话零件的派生检索索引（FTS5）。
//!
//! 这是**只读派生产物**，永远不参与事实事务：事实落库成功后由派生索引消费者按会话替换，
//! 因此索引维护失败不会回滚或阻塞会话同步（ADR-0023）。
//!
//! 索引正文由 [`crate::backend::domain::conversations::search_corpus`] 的版本化策略统一生成，
//! 只放行提问、最终回答、变更文件路径与命令语义标签；终端输出、diff 正文、推理日志与委派
//! 明细一概不入库。独立搜索索引消费同一份文档，两个后端因此共享同一套信号口径。

use super::*;
use crate::backend::domain::conversations::search_corpus::{
    current_search_corpus_policy, CorpusTurnFacts, SearchCorpusDocument, SearchCorpusPolicy,
};
use crate::backend::domain::conversations::triad::TriadPart;
use crate::backend::store::conversations::snapshot::restore_turn_parts;

/// 一个轮次已提交的事实投影，供语料抽取复用。
pub(crate) struct SessionCorpusTurn {
    pub turn_id: String,
    pub prompt: String,
    pub response: Option<String>,
    pub parts: Vec<crate::backend::domain::ConversationPart>,
}

impl SessionCorpusTurn {
    fn facts(&self) -> CorpusTurnFacts<'_> {
        CorpusTurnFacts {
            turn_id: &self.turn_id,
            prompt: &self.prompt,
            response: self.response.as_deref(),
            parts: &self.parts,
        }
    }
}

async fn load_session_adapter_id_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    session_id: &str,
    record_kind: ConversationRecordKind,
) -> StoreResult<Option<String>> {
    let query = format!(
        "SELECT adapter_id FROM {sessions} WHERE tenant_id = ?1 AND id = ?2",
        sessions = record_kind.tables().sessions,
    );
    sqlx::query_scalar::<_, String>(AssertSqlSafe(query))
        .bind(tenant_id)
        .bind(session_id)
        .fetch_optional(pool)
        .await
        .map_err(StoreError::external)
}

/// 读取一个会话已提交的轮次与零件事实，并还原成完整 Part 序列。
pub(crate) async fn load_session_corpus_turns_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    session_id: &str,
    record_kind: ConversationRecordKind,
) -> StoreResult<Vec<SessionCorpusTurn>> {
    let Some(adapter_id) =
        load_session_adapter_id_sqlx(pool, tenant_id, session_id, record_kind).await?
    else {
        return Ok(Vec::new());
    };
    let tables = record_kind.tables();
    let turn_rows = sqlx::query_as::<_, (String, String, Option<String>, Option<String>)>(
        AssertSqlSafe(format!(
            "SELECT id, prompt, process, response FROM {turns} \
             WHERE tenant_id = ?1 AND session_id = ?2 \
             ORDER BY turn_index ASC, id ASC",
            turns = tables.turns,
        )),
    )
    .bind(tenant_id)
    .bind(session_id)
    .fetch_all(pool)
    .await
    .map_err(StoreError::external)?;

    let part_columns = match record_kind {
        ConversationRecordKind::Session => {
            "p.turn_id, p.part_index, p.kind, p.content, p.attrs_json, p.translated_text"
        }
        ConversationRecordKind::Web => {
            "p.turn_id, p.part_index, p.kind, p.content, NULL AS attrs_json, p.translated_text"
        }
    };
    let part_rows: Vec<(
        String,
        i64,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
    )> = sqlx::query_as(AssertSqlSafe(format!(
        "SELECT {part_columns} \
         FROM {parts} p \
         JOIN {turns} t ON t.tenant_id = p.tenant_id AND t.id = p.turn_id \
         WHERE p.tenant_id = ?1 AND t.session_id = ?2 \
         ORDER BY p.turn_id ASC, p.part_index ASC",
        parts = tables.parts,
        turns = tables.turns,
        part_columns = part_columns,
    )))
    .bind(tenant_id)
    .bind(session_id)
    .fetch_all(pool)
    .await
    .map_err(StoreError::external)?;

    let mut parts_by_turn = BTreeMap::<String, Vec<TriadPart>>::new();
    for (turn_id, part_index, kind, content, attrs_json, translated_text) in part_rows {
        parts_by_turn.entry(turn_id).or_default().push(TriadPart {
            part_index,
            kind,
            content,
            attrs_json,
            translated_text,
        });
    }

    let prefix = match record_kind {
        ConversationRecordKind::Session => "conversation-part",
        ConversationRecordKind::Web => "web-record-part",
    };
    Ok(turn_rows
        .into_iter()
        .map(|(turn_id, prompt, process, response)| {
            let raw_parts = parts_by_turn.remove(&turn_id).unwrap_or_default();
            let parts = restore_turn_parts(
                &adapter_id,
                prefix,
                &turn_id,
                &prompt,
                process.as_deref(),
                response.as_deref(),
                &raw_parts,
            );
            SessionCorpusTurn {
                turn_id,
                prompt,
                response,
                parts,
            }
        })
        .collect())
}

/// 按策略把一个会话的事实压成后端中立文档。
pub(crate) fn build_session_corpus_documents(
    policy: &dyn SearchCorpusPolicy,
    turns: &[SessionCorpusTurn],
) -> Vec<SearchCorpusDocument> {
    let mut documents = Vec::new();
    for turn in turns {
        documents.extend(policy.extract(&turn.facts()));
    }
    documents
}

async fn insert_corpus_row_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    session_id: &str,
    document: &SearchCorpusDocument,
) -> StoreResult<()> {
    sqlx::query(
        r#"
        INSERT INTO conversation_part_fts (tenant_id, part_id, session_id, turn_id, text)
        VALUES (?1, ?2, ?3, ?4, ?5)
        "#,
    )
    .bind(tenant_id)
    .bind(document.anchor_id())
    .bind(session_id)
    .bind(&document.turn_id)
    .bind(&document.text)
    .execute(&mut **tx)
    .await
    .map_err(StoreError::external)?;
    Ok(())
}

/// 按会话幂等替换派生索引行：只写当前已提交快照的最新语料。
///
/// 小事务、按会话粒度、重复执行结果一致，因此失败重试不需要重导事实。
pub async fn replace_session_search_corpus_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    session_id: &str,
) -> StoreResult<usize> {
    let record_kind = resolve_record_kind_sqlx(pool, tenant_id, session_id)
        .await?
        .unwrap_or(ConversationRecordKind::Session);
    let turns = load_session_corpus_turns_sqlx(pool, tenant_id, session_id, record_kind).await?;
    let documents = build_session_corpus_documents(current_search_corpus_policy(), &turns);

    let mut tx = pool.begin().await.map_err(StoreError::external)?;
    sqlx::query("DELETE FROM conversation_part_fts WHERE tenant_id = ?1 AND session_id = ?2")
        .bind(tenant_id)
        .bind(session_id)
        .execute(&mut *tx)
        .await
        .map_err(StoreError::external)?;
    for document in &documents {
        insert_corpus_row_sqlx_tx(&mut tx, tenant_id, session_id, document).await?;
    }
    tx.commit().await.map_err(StoreError::external)?;
    Ok(documents.len())
}

/// 按源替换派生索引行。事件载荷未携带会话清单时用它兜底（超过 256 个会话的情形）。
pub async fn replace_source_search_corpus_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    source_id: &str,
) -> StoreResult<usize> {
    let mut indexed = 0usize;
    for record_kind in [ConversationRecordKind::Session, ConversationRecordKind::Web] {
        let query = format!(
            "SELECT id FROM {sessions} WHERE tenant_id = ?1 AND source_id = ?2 ORDER BY id ASC",
            sessions = record_kind.tables().sessions,
        );
        let session_ids = sqlx::query_scalar::<_, String>(AssertSqlSafe(query))
            .bind(tenant_id)
            .bind(source_id)
            .fetch_all(pool)
            .await
            .map_err(StoreError::external)?;
        for session_id in session_ids {
            indexed += replace_session_search_corpus_sqlx(pool, tenant_id, &session_id).await?;
        }
    }
    Ok(indexed)
}

pub(crate) async fn delete_session_part_index_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    session_id: &str,
) -> StoreResult<()> {
    sqlx::query("DELETE FROM conversation_part_fts WHERE tenant_id = ?1 AND session_id = ?2")
        .bind(tenant_id)
        .bind(session_id)
        .execute(&mut **tx)
        .await
        .map_err(StoreError::external)?;
    Ok(())
}

/// 按源清理某个表族的索引行（源下线 / 该表族记录被重建时使用）。
///
/// 只清理调用方真正删掉记录的那一族，避免把另一族仍然有效的索引行一起抹掉。
pub(crate) async fn delete_source_part_index_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    source_id: &str,
    record_kind: ConversationRecordKind,
) -> StoreResult<()> {
    let query = format!(
        r#"
        DELETE FROM conversation_part_fts
        WHERE tenant_id = ?1
          AND session_id IN (
              SELECT id FROM {sessions} WHERE tenant_id = ?1 AND source_id = ?2
          )
        "#,
        sessions = record_kind.tables().sessions,
    );
    sqlx::query(AssertSqlSafe(query))
        .bind(tenant_id)
        .bind(source_id)
        .execute(&mut **tx)
        .await
        .map_err(StoreError::external)?;
    Ok(())
}

pub(crate) async fn count_part_index_rows_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
) -> StoreResult<i64> {
    sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM conversation_part_fts WHERE tenant_id = ?1")
        .bind(tenant_id)
        .fetch_one(pool)
        .await
        .map_err(StoreError::external)
}

/// FTS5 段合并。只在整代重建成功后执行一次，避免增量同步承担整理成本。
pub(crate) async fn optimize_part_index_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
) -> StoreResult<()> {
    // FTS5 'optimize' 是整表操作，没有按租户切分的能力；多租户库里的成本仍然远低于
    // 每次增量同步的代价，且它只在本代重建收尾时触发。
    let _ = tenant_id;
    sqlx::query("INSERT INTO conversation_part_fts(conversation_part_fts) VALUES('optimize')")
        .execute(pool)
        .await
        .map_err(StoreError::external)?;
    Ok(())
}

async fn load_tenant_session_ids_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    record_kind: ConversationRecordKind,
) -> StoreResult<Vec<String>> {
    let query = format!(
        "SELECT id FROM {} WHERE tenant_id = ?1 ORDER BY id ASC",
        record_kind.tables().sessions,
    );
    sqlx::query_scalar::<_, String>(AssertSqlSafe(query))
        .bind(tenant_id)
        .fetch_all(pool)
        .await
        .map_err(StoreError::external)
}

/// 全量重建某租户的派生语料索引：按会话分事务推进，单会话失败不牵连整个重建。
///
/// 这就是语料策略升级后的受控重建入口：可中断、可重跑、可观测，且只在全部会话成功后
/// 才做一次段合并。
pub(crate) async fn rebuild_tenant_part_index_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    mut on_progress: impl FnMut(usize, usize),
) -> StoreResult<i64> {
    let mut session_ids = Vec::new();
    for record_kind in [ConversationRecordKind::Session, ConversationRecordKind::Web] {
        session_ids.extend(load_tenant_session_ids_sqlx(pool, tenant_id, record_kind).await?);
    }
    let total = session_ids.len();
    on_progress(0, total);
    for (index, session_id) in session_ids.iter().enumerate() {
        replace_session_search_corpus_sqlx(pool, tenant_id, session_id).await?;
        on_progress(index + 1, total);
    }
    optimize_part_index_sqlx(pool, tenant_id).await?;
    count_part_index_rows_sqlx(pool, tenant_id).await
}

/// 判断一个会话属于哪个表族：两族 id 前缀不同，但历史数据以落表为准。
pub(crate) async fn resolve_record_kind_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    session_id: &str,
) -> StoreResult<Option<ConversationRecordKind>> {
    let kind: Option<String> = sqlx::query_scalar(
        r#"
        SELECT 'session' AS record_kind
        FROM conversation_sessions
        WHERE tenant_id = ?1 AND id = ?2
        UNION ALL
        SELECT 'web' AS record_kind
        FROM web_record_sessions
        WHERE tenant_id = ?1 AND id = ?2
        LIMIT 1
        "#,
    )
    .bind(tenant_id)
    .bind(session_id)
    .fetch_optional(pool)
    .await
    .map_err(StoreError::external)?;
    Ok(kind.and_then(|value| match value.as_str() {
        "session" => Some(ConversationRecordKind::Session),
        "web" => Some(ConversationRecordKind::Web),
        _ => None,
    }))
}

/// 语料策略升级后的自动收敛闸门。
///
/// 存量索引是按旧口径生成的，必须整体重建成新一代才能继续对外提供正文匹配。这里只在
/// "口径落后且没有正在进行的构建 / 没有待排查的失败"时出手，避免与显式重建流水线打架，
/// 也避免把一次失败的重建变成事件派发热循环。返回 `true` 表示本租户已整体重建过。
pub async fn ensure_conversation_search_corpus_current_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
) -> StoreResult<bool> {
    let Some(state) = load_conversation_part_index_state_sqlx(pool, tenant_id).await? else {
        // 没有状态行 = 该租户从未建过索引；用当前口径建一代，后续增量只需按会话推进。
        let part_count = rebuild_tenant_part_index_sqlx(pool, tenant_id, |_, _| {}).await?;
        set_conversation_part_index_status_sqlx(
            pool,
            tenant_id,
            PART_INDEX_READY,
            part_count,
            None,
        )
        .await?;
        return Ok(true);
    };
    if state.is_ready() || state.status == PART_INDEX_BUILDING || state.last_error.is_some() {
        return Ok(false);
    }
    let part_count = rebuild_tenant_part_index_sqlx(pool, tenant_id, |_, _| {}).await?;
    set_conversation_part_index_status_sqlx(pool, tenant_id, PART_INDEX_READY, part_count, None)
        .await?;
    Ok(true)
}

#[cfg(test)]
#[path = "part_index_tests.rs"]
mod tests;
