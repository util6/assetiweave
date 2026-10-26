use super::*;
use sqlx::Row;

/// 把一张零件投影成它的索引正文。
///
/// 只取投影出来的正文，不对语义角色做任何判断——工具输出、执行结果、文件变更、子代理
/// 正文与答复、代码、命令走同一条路径。
pub(crate) fn part_index_text(
    part: &ConversationPart,
    adapter_id: &str,
    card_kinds: &[ConversationCardKindDefinition],
) -> StoreResult<Option<String>> {
    let cards =
        crate::backend::domain::conversations::projection::project_conversation_content_cards(
            part, adapter_id, card_kinds,
        )?;
    let text = cards
        .into_iter()
        .map(|card| card.body)
        .filter(|body| !body.trim().is_empty())
        .collect::<Vec<_>>()
        .join("\n\n");
    Ok((!text.trim().is_empty()).then_some(text))
}

async fn load_turn_session_id_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    turn_id: &str,
    record_kind: ConversationRecordKind,
) -> StoreResult<String> {
    let query = format!(
        "SELECT session_id FROM {} WHERE tenant_id = ?1 AND id = ?2",
        record_kind.tables().turns,
    );
    sqlx::query_scalar::<_, String>(AssertSqlSafe(query))
        .bind(tenant_id)
        .bind(turn_id)
        .fetch_one(&mut **tx)
        .await
        .map_err(StoreError::external)
}

async fn load_session_adapter_card_kinds_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    session_id: &str,
    record_kind: ConversationRecordKind,
) -> StoreResult<(String, Vec<ConversationCardKindDefinition>)> {
    let query = format!(
        r#"
        SELECT s.adapter_id, COALESCE(a.card_kinds_json, '[]') AS card_kinds_json
        FROM {sessions} s
        LEFT JOIN conversation_adapters a
          ON a.tenant_id = s.tenant_id AND a.id = s.adapter_id
        WHERE s.tenant_id = ?1 AND s.id = ?2
        "#,
        sessions = record_kind.tables().sessions,
    );
    let row = sqlx::query(AssertSqlSafe(query))
        .bind(tenant_id)
        .bind(session_id)
        .fetch_one(&mut **tx)
        .await
        .map_err(StoreError::external)?;
    let adapter_id: String = row.try_get("adapter_id").map_err(StoreError::external)?;
    let card_kinds_json: String = row
        .try_get("card_kinds_json")
        .map_err(StoreError::external)?;
    let card_kinds: Vec<ConversationCardKindDefinition> = decode_json(card_kinds_json)?;
    Ok((adapter_id, card_kinds))
}

async fn load_turn_parts_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    turn_id: &str,
    record_kind: ConversationRecordKind,
) -> StoreResult<Vec<ConversationPart>> {
    let tables = record_kind.tables();
    let turn_row = sqlx::query_as::<_, (String, Option<String>, Option<String>, String)>(
        AssertSqlSafe(format!(
            "SELECT t.prompt, t.process, t.response, s.adapter_id \
             FROM {turns} t \
             JOIN {sessions} s ON s.tenant_id = t.tenant_id AND s.id = t.session_id \
             WHERE t.tenant_id = ?1 AND t.id = ?2",
            turns = tables.turns,
            sessions = tables.sessions,
        )),
    )
    .bind(tenant_id)
    .bind(turn_id)
    .fetch_optional(&mut **tx)
    .await
    .map_err(StoreError::external)?;

    let Some((prompt, process_json, response, adapter_id)) = turn_row else {
        return Ok(Vec::new());
    };

    let part_columns = match record_kind {
        ConversationRecordKind::Session => {
            "p.part_index, p.kind, p.content, p.attrs_json, p.translated_text"
        }
        ConversationRecordKind::Web => {
            "p.part_index, p.kind, p.content, NULL AS attrs_json, p.translated_text"
        }
    };

    let part_rows: Vec<(i64, String, Option<String>, Option<String>, Option<String>)> =
        sqlx::query_as(AssertSqlSafe(format!(
            "SELECT {part_columns} \
             FROM {parts} p \
             WHERE p.tenant_id = ?1 AND p.turn_id = ?2 \
             ORDER BY p.part_index ASC",
            parts = tables.parts,
            part_columns = part_columns,
        )))
        .bind(tenant_id)
        .bind(turn_id)
        .fetch_all(&mut **tx)
        .await
        .map_err(StoreError::external)?;

    let triad_parts: Vec<crate::backend::domain::conversations::triad::TriadPart> = part_rows
        .into_iter()
        .map(|(part_index, kind, content, attrs_json, translated_text)| {
            crate::backend::domain::conversations::triad::TriadPart {
                part_index,
                kind,
                content,
                attrs_json,
                translated_text,
            }
        })
        .collect();

    let process: Option<crate::backend::domain::conversations::triad::TurnProcess> = process_json
        .as_deref()
        .and_then(|s| serde_json::from_str(s).ok());

    let prefix = match record_kind {
        ConversationRecordKind::Session => "conversation-part",
        ConversationRecordKind::Web => "web-record-part",
    };

    Ok(
        crate::backend::domain::conversations::triad::merge_turn_triad(
            &adapter_id,
            prefix,
            turn_id,
            &prompt,
            process.as_ref(),
            response.as_deref(),
            &triad_parts,
        ),
    )
}

async fn load_session_parts_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    session_id: &str,
    record_kind: ConversationRecordKind,
) -> StoreResult<Vec<ConversationPart>> {
    let tables = record_kind.tables();
    let turn_rows: Vec<(String, String, Option<String>, Option<String>, String)> =
        sqlx::query_as(AssertSqlSafe(format!(
            "SELECT t.id, t.prompt, t.process, t.response, s.adapter_id \
             FROM {turns} t \
             JOIN {sessions} s ON s.tenant_id = t.tenant_id AND s.id = t.session_id \
             WHERE t.tenant_id = ?1 AND t.session_id = ?2 \
             ORDER BY t.turn_index ASC, t.id ASC",
            turns = tables.turns,
            sessions = tables.sessions,
        )))
        .bind(tenant_id)
        .bind(session_id)
        .fetch_all(&mut **tx)
        .await
        .map_err(StoreError::external)?;

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
        part_columns = tables.part_columns,
    )))
    .bind(tenant_id)
    .bind(session_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(StoreError::external)?;

    let mut triad_parts_by_turn: BTreeMap<
        String,
        Vec<crate::backend::domain::conversations::triad::TriadPart>,
    > = BTreeMap::new();
    for (turn_id, part_index, kind, content, attrs_json, translated_text) in part_rows {
        triad_parts_by_turn.entry(turn_id).or_default().push(
            crate::backend::domain::conversations::triad::TriadPart {
                part_index,
                kind,
                content,
                attrs_json,
                translated_text,
            },
        );
    }

    let prefix = match record_kind {
        ConversationRecordKind::Session => "conversation-part",
        ConversationRecordKind::Web => "web-record-part",
    };

    let mut all_parts = Vec::new();
    for (turn_id, prompt, process_json, response, adapter_id) in turn_rows {
        let empty = Vec::new();
        let turn_parts = triad_parts_by_turn.get(&turn_id).unwrap_or(&empty);
        let process: Option<crate::backend::domain::conversations::triad::TurnProcess> =
            process_json
                .as_deref()
                .and_then(|s| serde_json::from_str(s).ok());
        let restored = crate::backend::domain::conversations::triad::merge_turn_triad(
            &adapter_id,
            prefix,
            &turn_id,
            &prompt,
            process.as_ref(),
            response.as_deref(),
            turn_parts,
        );
        all_parts.extend(restored);
    }
    Ok(all_parts)
}

async fn insert_part_index_row_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    part: &ConversationPart,
    session_id: &str,
    text: &str,
) -> StoreResult<()> {
    sqlx::query(
        r#"
        INSERT INTO conversation_part_fts (tenant_id, part_id, session_id, turn_id, text)
        VALUES (?1, ?2, ?3, ?4, ?5)
        "#,
    )
    .bind(tenant_id)
    .bind(&part.id)
    .bind(session_id)
    .bind(&part.turn_id)
    .bind(text)
    .execute(&mut **tx)
    .await
    .map_err(StoreError::external)?;
    Ok(())
}

pub(crate) async fn delete_turn_part_index_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    turn_id: &str,
) -> StoreResult<()> {
    sqlx::query("DELETE FROM conversation_part_fts WHERE tenant_id = ?1 AND turn_id = ?2")
        .bind(tenant_id)
        .bind(turn_id)
        .execute(&mut **tx)
        .await
        .map_err(StoreError::external)?;
    Ok(())
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

/// 随零件写入增量维护索引：一个轮次的零件整体替换时，只重建该轮次的索引行。
///
/// 索引只依赖零件本身（正文、命令、投影契约），与问题分组无关，因此合并 / 拆分、
/// 问题聚合重建都不需要触碰索引。
pub(crate) async fn replace_turn_part_index_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    turn_id: &str,
    record_kind: ConversationRecordKind,
) -> StoreResult<usize> {
    let session_id = load_turn_session_id_sqlx_tx(tx, tenant_id, turn_id, record_kind).await?;
    delete_turn_part_index_sqlx_tx(tx, tenant_id, turn_id).await?;
    let (adapter_id, card_kinds) =
        load_session_adapter_card_kinds_sqlx_tx(tx, tenant_id, &session_id, record_kind).await?;
    let mut indexed = 0usize;
    for part in load_turn_parts_sqlx_tx(tx, tenant_id, turn_id, record_kind).await? {
        let Some(text) = part_index_text(&part, &adapter_id, &card_kinds)? else {
            continue;
        };
        insert_part_index_row_sqlx_tx(tx, tenant_id, &part, &session_id, &text).await?;
        indexed += 1;
    }
    Ok(indexed)
}

/// 按会话重建零件索引行：先清掉该会话的旧行，再按当前零件逐行写入。
pub(crate) async fn replace_session_part_index_sqlx_tx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    session_id: &str,
    record_kind: ConversationRecordKind,
) -> StoreResult<usize> {
    delete_session_part_index_sqlx_tx(tx, tenant_id, session_id).await?;
    let (adapter_id, card_kinds) =
        load_session_adapter_card_kinds_sqlx_tx(tx, tenant_id, session_id, record_kind).await?;
    let mut indexed = 0usize;
    for part in load_session_parts_sqlx_tx(tx, tenant_id, session_id, record_kind).await? {
        let Some(text) = part_index_text(&part, &adapter_id, &card_kinds)? else {
            continue;
        };
        insert_part_index_row_sqlx_tx(tx, tenant_id, &part, session_id, &text).await?;
        indexed += 1;
    }
    Ok(indexed)
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

/// 全量重建某租户的零件索引：按会话分事务推进，单会话失败不牵连整个重建。
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
        let record_kind = resolve_record_kind_sqlx(pool, tenant_id, session_id)
            .await?
            .unwrap_or(ConversationRecordKind::Session);
        let mut tx = pool.begin().await.map_err(StoreError::external)?;
        replace_session_part_index_sqlx_tx(&mut tx, tenant_id, session_id, record_kind).await?;
        tx.commit().await.map_err(StoreError::external)?;
        on_progress(index + 1, total);
    }
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

#[cfg(test)]
#[path = "part_index_tests.rs"]
mod tests;
