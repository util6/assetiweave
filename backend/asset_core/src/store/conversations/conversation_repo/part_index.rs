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
    // 必须以轮次驱动、并用 CROSS JOIN 固定连接顺序。
    //
    // 反过来写（零件驱动 + ORDER BY p.turn_id）会让规划器为了省一次排序而选择
    // "按 tenant_id 扫描全部零件"，单会话就要数秒；固定成 turns → parts 后走
    // (tenant_id, session_id) 与 (tenant_id, turn_id) 两个索引，实测快 60 倍。
    let part_rows: Vec<(
        String,
        i64,
        String,
        Option<String>,
        Option<String>,
        Option<String>,
    )> = sqlx::query_as(AssertSqlSafe(format!(
        "SELECT {part_columns} \
         FROM {turns} t \
         CROSS JOIN {parts} p ON p.tenant_id = t.tenant_id AND p.turn_id = t.id \
         WHERE t.tenant_id = ?1 AND t.session_id = ?2 \
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
    let inserted = sqlx::query(
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
    // FTS5 的元数据列是 UNINDEXED：按 session_id 删除会退化成"扫描全部正文"，
    // 单会话就要十秒级。落一行 rowid 映射，删除才能保持点查。
    sqlx::query(
        r#"
        INSERT INTO conversation_search_doc_row (fts_rowid, tenant_id, session_id)
        VALUES (?1, ?2, ?3)
        "#,
    )
    .bind(inserted.last_insert_rowid())
    .bind(tenant_id)
    .bind(session_id)
    .execute(&mut **tx)
    .await
    .map_err(StoreError::external)?;
    Ok(())
}

/// 按会话精确删除派生索引行。
///
/// 只能走 rowid 映射：FTS5 的 `tenant_id` / `session_id` 是 UNINDEXED 列，
/// 直接按它们过滤会扫描全部正文（实测单次 8~19 秒），这正是增量同步被拖慢的根因。
async fn delete_session_search_corpus_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    session_id: &str,
) -> StoreResult<()> {
    let rowids = sqlx::query_scalar::<_, i64>(
        r#"
        SELECT fts_rowid FROM conversation_search_doc_row
        WHERE tenant_id = ?1 AND session_id = ?2
        "#,
    )
    .bind(tenant_id)
    .bind(session_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(StoreError::external)?;
    for rowid in rowids {
        sqlx::query("DELETE FROM conversation_part_fts WHERE rowid = ?1")
            .bind(rowid)
            .execute(&mut **tx)
            .await
            .map_err(StoreError::external)?;
    }
    sqlx::query("DELETE FROM conversation_search_doc_row WHERE tenant_id = ?1 AND session_id = ?2")
        .bind(tenant_id)
        .bind(session_id)
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
    delete_session_search_corpus_sqlx_tx(&mut tx, tenant_id, session_id).await?;
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
    delete_session_search_corpus_sqlx_tx(tx, tenant_id, session_id).await
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
        "SELECT id FROM {sessions} WHERE tenant_id = ?1 AND source_id = ?2",
        sessions = record_kind.tables().sessions,
    );
    let session_ids = sqlx::query_scalar::<_, String>(AssertSqlSafe(query))
        .bind(tenant_id)
        .bind(source_id)
        .fetch_all(&mut **tx)
        .await
        .map_err(StoreError::external)?;
    for session_id in session_ids {
        delete_session_search_corpus_sqlx_tx(tx, tenant_id, &session_id).await?;
    }
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

    // 整代重建一次性清空本租户，而不是逐会话删除：逐会话删除会把旧代噪音反复扫一遍，
    // 2500 个会话就是 2500 次全表正文扫描。
    //
    // 清理前先补齐 rowid 映射：历史版本可能写过没有映射的索引行，映射缺行会让这些行
    // 既删不掉也搜不到，变成永久垃圾。补齐是一次平查，成本可忽略。
    reconcile_search_doc_row_map_sqlx(pool).await?;
    delete_tenant_search_corpus_sqlx(pool, tenant_id).await?;

    for (index, session_id) in session_ids.iter().enumerate() {
        insert_session_search_corpus_sqlx(pool, tenant_id, session_id).await?;
        on_progress(index + 1, total);
    }
    // 段合并只是体积优化，失败不应让一整代已经正确的索引判定为重建失败。
    if let Err(error) = optimize_part_index_sqlx(pool).await {
        tracing::warn!(
            action = "conversation.corpus_optimize",
            error = %error,
            "FTS segment optimization failed; the rebuilt generation is still valid"
        );
    }
    count_part_index_rows_sqlx(pool, tenant_id).await
}

/// 为缺少映射的索引行补登 rowid，保证按映射删除能覆盖全部历史行。
async fn reconcile_search_doc_row_map_sqlx(pool: &SqlitePool) -> StoreResult<()> {
    sqlx::query(
        r#"
        INSERT INTO conversation_search_doc_row (fts_rowid, tenant_id, session_id)
        SELECT f.rowid, f.tenant_id, f.session_id
        FROM conversation_part_fts f
        WHERE NOT EXISTS (
            SELECT 1 FROM conversation_search_doc_row m WHERE m.fts_rowid = f.rowid
        )
        "#,
    )
    .execute(pool)
    .await
    .map_err(StoreError::external)?;
    Ok(())
}

/// 分批清空本租户的派生索引行。
///
/// 一次性 `DELETE ... WHERE tenant_id = ?` 会在单个事务里持有写锁近十秒（实测 26 万行 9.5 秒），
/// 期间任何同步写入都会撞上 `database is locked`。这里按 rowid 分块提交，
/// 把写锁占用压到毫秒级，整代重建因此可以与其他任务共存。
async fn delete_tenant_search_corpus_sqlx(pool: &SqlitePool, tenant_id: &str) -> StoreResult<()> {
    const CHUNK: i64 = 2000;
    loop {
        // 不要在这里 ORDER BY：按 (tenant_id, session_id) 索引前缀取一批就行了，
        // 排序会把每批都变成对全表的一次全量排序。
        let rowids = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT fts_rowid FROM conversation_search_doc_row
            WHERE tenant_id = ?1 LIMIT ?2
            "#,
        )
        .bind(tenant_id)
        .bind(CHUNK)
        .fetch_all(pool)
        .await
        .map_err(StoreError::external)?;
        if rowids.is_empty() {
            return Ok(());
        }
        let mut tx = pool.begin().await.map_err(StoreError::external)?;
        for rowid in &rowids {
            sqlx::query("DELETE FROM conversation_part_fts WHERE rowid = ?1")
                .bind(rowid)
                .execute(&mut *tx)
                .await
                .map_err(StoreError::external)?;
            sqlx::query("DELETE FROM conversation_search_doc_row WHERE fts_rowid = ?1")
                .bind(rowid)
                .execute(&mut *tx)
                .await
                .map_err(StoreError::external)?;
        }
        tx.commit().await.map_err(StoreError::external)?;
    }
}

/// 只写入（不删除）某个会话的当前语料，供整代重建使用。
async fn insert_session_search_corpus_sqlx(
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
    for document in &documents {
        insert_corpus_row_sqlx_tx(&mut tx, tenant_id, session_id, document).await?;
    }
    tx.commit().await.map_err(StoreError::external)?;
    Ok(documents.len())
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

/// 当前索引代际是否落后于生效的语料策略。
///
/// 只做判定，不在这里重建：整体重建必须走受阶段化的后台流水线（ADR-0017），
/// 否则一次失败的自动收敛会把索引永久钉在旧代际上，并与显式重建互相打架。
pub async fn conversation_search_corpus_needs_rebuild_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
) -> StoreResult<bool> {
    Ok(load_conversation_part_index_state_sqlx(pool, tenant_id)
        .await?
        .map(|state| !state.is_ready())
        .unwrap_or(false))
}

/// FTS5 段合并。
///
/// FTS5 的 `optimize` 是整表操作，没有按租户切分的能力；因此它只在整代重建收尾时
/// 触发一次，日常增量永不承担整理成本。
pub(crate) async fn optimize_part_index_sqlx(pool: &SqlitePool) -> StoreResult<()> {
    sqlx::query("INSERT INTO conversation_part_fts(conversation_part_fts) VALUES('optimize')")
        .execute(pool)
        .await
        .map_err(StoreError::external)?;
    Ok(())
}

#[cfg(test)]
#[path = "part_index_tests.rs"]
mod tests;
