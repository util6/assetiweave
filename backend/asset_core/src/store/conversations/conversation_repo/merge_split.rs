use super::*;
use sqlx::QueryBuilder;

/// 合并：把多组轮次改写为同一分组标识，并把来源置为手动。
///
/// 分组事实只落在轮次表的两列上，旧的问题表与成员表不再参与；被并掉的标识只写进
/// 跳转表墓碑，使合并前的链接继续解析到幸存分组。dry-run 在任何写语句之前返回。
pub async fn merge_conversation_questions_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    question_ids: &[String],
    dry_run: bool,
) -> StoreResult<ConversationMutationResult> {
    if question_ids.len() < 2 {
        return Err(StoreError::Validation(
            "at least two question ids are required".to_string(),
        ));
    }

    let mut tx = pool.begin().await.map_err(StoreError::external)?;
    let targets = resolve_merge_targets_sqlx(&mut tx, tenant_id, question_ids).await?;
    let session_id = targets.session_id;
    let canonical_ids = targets.question_ids;

    if canonical_ids.len() < 2 {
        tx.rollback().await.map_err(StoreError::external)?;
        return Ok(ConversationMutationResult {
            dry_run,
            session_id,
            affected_question_ids: question_ids.to_vec(),
            questions: vec![
                load_conversation_question_detail_sqlx(pool, tenant_id, &canonical_ids[0]).await?,
            ],
        });
    }
    ensure_merge_span_is_contiguous_sqlx(&mut tx, tenant_id, &session_id, &canonical_ids).await?;

    if dry_run {
        tx.rollback().await.map_err(StoreError::external)?;
        let mut details = Vec::with_capacity(canonical_ids.len());
        for question_id in &canonical_ids {
            details
                .push(load_conversation_question_detail_sqlx(pool, tenant_id, question_id).await?);
        }
        return Ok(ConversationMutationResult {
            dry_run: true,
            session_id,
            affected_question_ids: question_ids.to_vec(),
            questions: details,
        });
    }

    let survivor_id = canonical_ids[0].clone();
    let now = Utc::now().to_rfc3339();
    reassign_turn_question_ids_sqlx(
        &mut tx,
        tenant_id,
        &session_id,
        &canonical_ids,
        &survivor_id,
    )
    .await?;
    write_merge_tombstones_sqlx(&mut tx, tenant_id, &canonical_ids[1..], &survivor_id, &now)
        .await?;
    tx.commit().await.map_err(StoreError::external)?;

    Ok(ConversationMutationResult {
        dry_run: false,
        session_id,
        affected_question_ids: question_ids.to_vec(),
        questions: vec![
            load_conversation_question_detail_sqlx(pool, tenant_id, &survivor_id).await?,
        ],
    })
}

/// 拆分：把指定轮次及其之后的轮次划给新的分组标识。
///
/// 源分组保留在它自己的标识上，整组的来源一并升级为手动，使自动归并不会把拆开的
/// 两段重新粘回去。dry-run 在任何写语句之前返回。
pub async fn split_conversation_question_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    question_id: &str,
    before_turn_id: &str,
    dry_run: bool,
) -> StoreResult<ConversationMutationResult> {
    let mut tx = pool.begin().await.map_err(StoreError::external)?;
    let turns = load_turn_group_positions_sqlx(&mut tx, tenant_id, question_id).await?;
    let Some(first_turn) = turns.first() else {
        return Err(StoreError::external(format!(
            "conversation question not found: {question_id}"
        )));
    };
    let session_id = first_turn.session_id.clone();
    let new_question_id = stable_id(
        "conversation-question",
        &["split", question_id, before_turn_id],
    );

    let Some(split_index) = turns.iter().position(|turn| turn.id == before_turn_id) else {
        let existing_question_id = sqlx::query_scalar::<_, String>(
            r#"
            SELECT question_id
            FROM conversation_turns
            WHERE tenant_id = ?1 AND session_id = ?2 AND id = ?3
            "#,
        )
        .bind(tenant_id)
        .bind(&session_id)
        .bind(before_turn_id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(StoreError::external)?;
        if existing_question_id.as_deref() == Some(new_question_id.as_str()) {
            tx.rollback().await.map_err(StoreError::external)?;
            return Ok(ConversationMutationResult {
                dry_run,
                session_id,
                affected_question_ids: vec![question_id.to_string(), new_question_id.clone()],
                questions: vec![
                    load_conversation_question_detail_sqlx(pool, tenant_id, question_id).await?,
                    load_conversation_question_detail_sqlx(pool, tenant_id, &new_question_id)
                        .await?,
                ],
            });
        }
        return Err(StoreError::external(format!(
            "turn is not in question: {before_turn_id}"
        )));
    };
    if split_index == 0 {
        return Err(StoreError::Validation(
            "split turn must not be the first turn in the question".to_string(),
        ));
    }
    let split_turn_index = turns[split_index].turn_index;

    if dry_run {
        tx.rollback().await.map_err(StoreError::external)?;
        return Ok(ConversationMutationResult {
            dry_run: true,
            session_id,
            affected_question_ids: vec![question_id.to_string()],
            questions: vec![
                load_conversation_question_detail_sqlx(pool, tenant_id, question_id).await?,
            ],
        });
    }

    sqlx::query(
        r#"
        UPDATE conversation_turns
        SET question_id = CASE WHEN turn_index >= ?1 THEN ?2 ELSE question_id END,
            question_origin = ?3
        WHERE tenant_id = ?4 AND session_id = ?5 AND question_id = ?6
        "#,
    )
    .bind(split_turn_index)
    .bind(&new_question_id)
    .bind(encode_enum(ConversationGroupingOrigin::Manual)?)
    .bind(tenant_id)
    .bind(&session_id)
    .bind(question_id)
    .execute(&mut *tx)
    .await
    .map_err(StoreError::external)?;
    tx.commit().await.map_err(StoreError::external)?;

    Ok(ConversationMutationResult {
        dry_run: false,
        session_id,
        affected_question_ids: vec![question_id.to_string(), new_question_id.clone()],
        questions: vec![
            load_conversation_question_detail_sqlx(pool, tenant_id, question_id).await?,
            load_conversation_question_detail_sqlx(pool, tenant_id, &new_question_id).await?,
        ],
    })
}

struct ResolvedMergeTargets {
    session_id: String,
    question_ids: Vec<String>,
}

/// 解析墓碑并确认所有分组属于同一会话，返回去重后的分组标识（保持传入顺序）。
async fn resolve_merge_targets_sqlx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    question_ids: &[String],
) -> StoreResult<ResolvedMergeTargets> {
    let mut session_id: Option<String> = None;
    let mut unique_ids = Vec::with_capacity(question_ids.len());
    for requested_id in question_ids {
        let resolved_id =
            resolve_conversation_question_redirect_sqlx(&mut **tx, tenant_id, requested_id).await?;
        let question_session_id = sqlx::query_scalar::<_, String>(
            "SELECT session_id FROM conversation_turns WHERE tenant_id = ?1 AND question_id = ?2 LIMIT 1",
        )
        .bind(tenant_id)
        .bind(&resolved_id)
        .fetch_optional(&mut **tx)
        .await
        .map_err(StoreError::external)?;
        let Some(question_session_id) = question_session_id else {
            return Err(StoreError::external(format!(
                "conversation question not found: {requested_id}"
            )));
        };
        match &session_id {
            Some(existing) if existing != &question_session_id => {
                return Err(StoreError::Validation(
                    "questions must belong to the same session".to_string(),
                ));
            }
            Some(_) => {}
            None => session_id = Some(question_session_id),
        }
        if !unique_ids.contains(&resolved_id) {
            unique_ids.push(resolved_id);
        }
    }
    Ok(ResolvedMergeTargets {
        session_id: session_id.unwrap_or_default(),
        question_ids: unique_ids,
    })
}

/// 相邻性约束落到轮次序号上：被合并的分组必须正好铺满一段连续的轮次区间，
/// 中间不能夹着其他分组，也不能留下序号空洞。
async fn ensure_merge_span_is_contiguous_sqlx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    session_id: &str,
    question_ids: &[String],
) -> StoreResult<()> {
    let mut groups = QueryBuilder::<Sqlite>::new(
        "SELECT question_id FROM conversation_turns WHERE tenant_id = ",
    );
    groups.push_bind(tenant_id.to_string());
    groups.push(" AND session_id = ");
    groups.push_bind(session_id.to_string());
    groups.push(" AND question_id IN (");
    push_question_id_list(&mut groups, question_ids);
    groups.push(") GROUP BY question_id ORDER BY MIN(turn_index) ASC, question_id ASC");
    let ordered_ids = groups
        .build_query_scalar::<String>()
        .fetch_all(&mut **tx)
        .await
        .map_err(StoreError::external)?;
    if ordered_ids.len() != question_ids.len() {
        return Err(StoreError::Validation(
            "all questions must exist in the session".to_string(),
        ));
    }
    if ordered_ids
        .iter()
        .zip(question_ids.iter())
        .any(|(actual, requested)| actual != requested)
    {
        return Err(StoreError::Validation(
            "question ids must be supplied in session order".to_string(),
        ));
    }

    let mut span = QueryBuilder::<Sqlite>::new(
        "SELECT MIN(turn_index), MAX(turn_index), COUNT(*) \
         FROM conversation_turns WHERE tenant_id = ",
    );
    span.push_bind(tenant_id.to_string());
    span.push(" AND session_id = ");
    span.push_bind(session_id.to_string());
    span.push(" AND question_id IN (");
    push_question_id_list(&mut span, question_ids);
    span.push(")");
    let (first_turn_index, last_turn_index, selected_turn_count) = span
        .build_query_as::<(i64, i64, i64)>()
        .fetch_one(&mut **tx)
        .await
        .map_err(StoreError::external)?;

    // 序号跨度必须正好等于被选中的轮次数，否则序号中间有空洞。
    if last_turn_index - first_turn_index + 1 != selected_turn_count {
        return Err(StoreError::Validation(
            "questions must be adjacent in turn order".to_string(),
        ));
    }

    // 区间内的轮次数多于被选中的轮次数，说明中间夹了别的分组。
    let span_turn_count: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*)
        FROM conversation_turns
        WHERE tenant_id = ?1 AND session_id = ?2 AND turn_index >= ?3 AND turn_index <= ?4
        "#,
    )
    .bind(tenant_id)
    .bind(session_id)
    .bind(first_turn_index)
    .bind(last_turn_index)
    .fetch_one(&mut **tx)
    .await
    .map_err(StoreError::external)?;
    if span_turn_count != selected_turn_count {
        return Err(StoreError::Validation(
            "questions must be adjacent in turn order".to_string(),
        ));
    }
    Ok(())
}

fn push_question_id_list(builder: &mut QueryBuilder<Sqlite>, question_ids: &[String]) {
    let mut separated = builder.separated(", ");
    for question_id in question_ids {
        separated.push_bind(question_id.clone());
    }
}

/// 单表更新：把给定分组的轮次统一改写到目标标识，并把来源置为手动。
async fn reassign_turn_question_ids_sqlx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    session_id: &str,
    question_ids: &[String],
    target_id: &str,
) -> StoreResult<u64> {
    let mut query = QueryBuilder::<Sqlite>::new("UPDATE conversation_turns SET question_id = ");
    query.push_bind(target_id.to_string());
    query.push(", question_origin = ");
    query.push_bind(encode_enum(ConversationGroupingOrigin::Manual)?);
    query.push(" WHERE tenant_id = ");
    query.push_bind(tenant_id.to_string());
    query.push(" AND session_id = ");
    query.push_bind(session_id.to_string());
    query.push(" AND question_id IN (");
    push_question_id_list(&mut query, question_ids);
    query.push(")");
    let result = query
        .build()
        .execute(&mut **tx)
        .await
        .map_err(StoreError::external)?;
    Ok(result.rows_affected())
}

/// 被合并掉的标识只留墓碑：既有墓碑改指向幸存分组，再为本次被并掉的标识补一条。
async fn write_merge_tombstones_sqlx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    merged_away_ids: &[String],
    survivor_id: &str,
    now: &str,
) -> StoreResult<()> {
    for merged_away_id in merged_away_ids {
        sqlx::query(
            r#"
            UPDATE conversation_question_redirects
            SET target_question_id = ?1, updated_at = ?2
            WHERE tenant_id = ?3 AND target_question_id = ?4
            "#,
        )
        .bind(survivor_id)
        .bind(now)
        .bind(tenant_id)
        .bind(merged_away_id)
        .execute(&mut **tx)
        .await
        .map_err(StoreError::external)?;
        sqlx::query(
            r#"
            INSERT INTO conversation_question_redirects (
                tenant_id, source_question_id, target_question_id,
                operation_kind, created_at, updated_at
            )
            VALUES (?1, ?2, ?3, 'merge', ?4, ?4)
            ON CONFLICT(tenant_id, source_question_id) DO UPDATE SET
                target_question_id = excluded.target_question_id,
                operation_kind = excluded.operation_kind,
                updated_at = excluded.updated_at
            "#,
        )
        .bind(tenant_id)
        .bind(merged_away_id)
        .bind(survivor_id)
        .bind(now)
        .execute(&mut **tx)
        .await
        .map_err(StoreError::external)?;
    }
    Ok(())
}

#[derive(Debug, FromRow)]
struct TurnGroupPositionRow {
    id: String,
    session_id: String,
    turn_index: i64,
}

async fn load_turn_group_positions_sqlx(
    tx: &mut Transaction<'_, Sqlite>,
    tenant_id: &str,
    question_id: &str,
) -> StoreResult<Vec<TurnGroupPositionRow>> {
    sqlx::query_as::<_, TurnGroupPositionRow>(
        r#"
        SELECT id, session_id, turn_index
        FROM conversation_turns
        WHERE tenant_id = ?1 AND question_id = ?2
        ORDER BY turn_index ASC, id ASC
        "#,
    )
    .bind(tenant_id)
    .bind(question_id)
    .fetch_all(&mut **tx)
    .await
    .map_err(StoreError::external)
}
