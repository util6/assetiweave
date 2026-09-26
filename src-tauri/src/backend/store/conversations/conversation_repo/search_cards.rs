use super::*;

pub(crate) async fn load_recent_conversation_sync_deltas_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    record_kind: ConversationRecordKind,
    source_id: Option<&str>,
    adapter_id: Option<&str>,
    recent_run_limit: usize,
) -> StoreResult<Vec<ConversationSyncDelta>> {
    let record_kind = match record_kind {
        ConversationRecordKind::Session => "session",
        ConversationRecordKind::Web => "web",
    };
    let run_limit = i64::try_from(recent_run_limit.clamp(1, 20)).map_err(|_| {
        StoreError::external("invalid recent conversation sync run limit".to_string())
    })?;
    sqlx::query_as::<_, ConversationSyncDelta>(
        r#"
        WITH recent_runs AS (
            SELECT r.id
            FROM conversation_sync_runs r
            JOIN conversation_sync_deltas d
              ON d.tenant_id = r.tenant_id AND d.sync_run_id = r.id
            WHERE r.tenant_id = ?1
              AND r.status = 'completed'
              AND d.record_kind = ?2
              AND (?3 IS NULL OR r.source_id = ?3)
              AND (?4 IS NULL OR r.adapter_id = ?4)
            GROUP BY r.id
            ORDER BY MAX(d.observed_at) DESC, r.id DESC
            LIMIT ?5
        )
        SELECT d.sync_run_id, d.session_id, d.change_kind, d.observed_at
        FROM conversation_sync_deltas d
        JOIN recent_runs r ON r.id = d.sync_run_id
        WHERE d.tenant_id = ?1 AND d.record_kind = ?2
        ORDER BY d.observed_at DESC, d.sync_run_id DESC, d.session_id ASC
        "#,
    )
    .bind(tenant_id)
    .bind(record_kind)
    .bind(source_id)
    .bind(adapter_id)
    .bind(run_limit)
    .fetch_all(pool)
    .await
    .map_err(StoreError::external)
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn search_conversation_cards_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    record_kind: ConversationRecordKind,
    adapter_id: Option<&str>,
    source_id: Option<&str>,
    project_path: Option<&str>,
    query: &str,
    content_types: &[ConversationSearchCardType],
    semantic_roles: &[String],
    include_questions: bool,
    include_cards: bool,
    since: Option<&str>,
    until: Option<&str>,
    timeline: bool,
    limit: usize,
    offset: usize,
    allowed_session_ids: Option<&BTreeSet<String>>,
) -> StoreResult<ConversationSearchPage> {
    let needle = normalize_query(Some(query))
        .ok_or_else(|| StoreError::external("conversation search query is required".to_string()))?;
    let id_fragment = crate::backend::domain::conversation_id_search_term(query)
        .map(|value| crate::backend::domain::conversation_id_fragment(&value));
    let project_path = normalize_project_path(project_path);
    let since = parse_search_time_bound(since, SearchTimeBound::Since)?;
    let until = parse_search_time_bound(until, SearchTimeBound::Until)?;
    let allowed_types = content_types.iter().cloned().collect::<BTreeSet<_>>();
    let allowed_semantic_roles = semantic_roles.iter().cloned().collect::<BTreeSet<_>>();
    let all_types = BTreeSet::new();
    let adapter_card_kinds = load_search_adapter_card_kinds_sqlx(pool, tenant_id).await?;
    let tables = record_kind.tables();
    let id_matched_session_ids = if let Some(fragment) = id_fragment.as_deref() {
        let session_ids =
            load_search_session_ids_by_id_fragment_sqlx(pool, tenant_id, tables, fragment).await?;
        if session_ids.is_empty() {
            return Ok(ConversationSearchPage {
                total_count: 0,
                hits: Vec::new(),
            });
        }
        Some(session_ids)
    } else {
        None
    };
    let selected_session_ids = match (id_matched_session_ids, allowed_session_ids) {
        (Some(id_matched), Some(allowed)) => id_matched
            .intersection(allowed)
            .cloned()
            .collect::<BTreeSet<_>>(),
        (Some(id_matched), None) => id_matched,
        (None, Some(allowed)) => allowed.clone(),
        (None, None) => BTreeSet::new(),
    };
    if (id_fragment.is_some() || allowed_session_ids.is_some()) && selected_session_ids.is_empty() {
        return Ok(ConversationSearchPage {
            total_count: 0,
            hits: Vec::new(),
        });
    }
    let session_ids_json = if id_fragment.is_some() || allowed_session_ids.is_some() {
        Some(serde_json::to_string(&selected_session_ids).map_err(StoreError::external)?)
    } else {
        None
    };
    let mut sessions = load_search_sessions_sqlx(
        pool,
        tenant_id,
        tables,
        adapter_id,
        source_id,
        session_ids_json.as_deref(),
    )
    .await?;
    if timeline {
        sessions.sort_by(|left, right| {
            conversation_session_search_time(&left.session)
                .cmp(&conversation_session_search_time(&right.session))
                .then_with(|| left.session.title.cmp(&right.session.title))
        });
    }
    let mut questions_by_session = load_search_questions_sqlx(
        pool,
        tenant_id,
        tables,
        adapter_id,
        source_id,
        session_ids_json.as_deref(),
    )
    .await?;
    let mut turns_by_question = load_search_turns_sqlx(
        pool,
        tenant_id,
        tables,
        adapter_id,
        source_id,
        session_ids_json.as_deref(),
    )
    .await?;
    let mut parts_by_turn = load_search_parts_sqlx(
        pool,
        tenant_id,
        tables,
        adapter_id,
        source_id,
        session_ids_json.as_deref(),
    )
    .await?;
    let mut hits = Vec::new();

    for session_item in sessions {
        let session = &session_item.session;
        if let Some(project_path) = project_path.as_deref() {
            let session_project = normalize_project_path(session.project_path.as_deref());
            if session_project.as_deref() != Some(project_path) {
                continue;
            }
        }
        if since.is_some() || until.is_some() {
            let Some(session_time) = conversation_session_search_time(session) else {
                continue;
            };
            if let Some(since) = since.as_ref() {
                if &session_time < since {
                    continue;
                }
            }
            if let Some(until) = until.as_ref() {
                if &session_time > until {
                    continue;
                }
            }
        }

        for (question_index, question) in questions_by_session
            .remove(&session.id)
            .unwrap_or_default()
            .into_iter()
            .enumerate()
        {
            let question_turns = turns_by_question.remove(&question.id).unwrap_or_default();
            let question_title = search_question_title_from_turns(&question, &question_turns);
            for turn in question_turns {
                let question_block_id = format!("{}-question", turn.id);
                if include_questions {
                    push_search_hit_if_matching(
                        &mut hits,
                        &needle,
                        &all_types,
                        &session_item,
                        &question,
                        question_index as i64,
                        &question_title,
                        Some(turn.id.clone()),
                        None,
                        question_block_id.clone(),
                        ConversationSearchCardType::question(),
                        &turn.user_text,
                        id_fragment.as_deref(),
                        &[&session.id, &question.id, &turn.id, &question_block_id],
                    );
                }

                if !include_cards {
                    continue;
                }
                for part in parts_by_turn.remove(&turn.id).unwrap_or_default() {
                    for entry in search_entries_for_part(
                        &part,
                        &session.adapter_id,
                        adapter_card_kinds
                            .get(&session.adapter_id)
                            .map(Vec::as_slice)
                            .unwrap_or_default(),
                    ) {
                        if !allowed_semantic_roles.is_empty()
                            && entry
                                .semantic_role
                                .as_ref()
                                .is_none_or(|role| !allowed_semantic_roles.contains(role))
                        {
                            continue;
                        }
                        let entry_block_id = entry.block_id.clone();
                        push_search_hit_if_matching(
                            &mut hits,
                            &needle,
                            &allowed_types,
                            &session_item,
                            &question,
                            question_index as i64,
                            &question_title,
                            Some(turn.id.clone()),
                            Some(part.id.clone()),
                            entry.block_id,
                            entry.card_type,
                            &entry.text,
                            id_fragment.as_deref(),
                            &[
                                &session.id,
                                &question.id,
                                &turn.id,
                                &part.id,
                                &entry_block_id,
                            ],
                        );
                    }
                }
            }
        }
    }

    let total_count = hits.len();
    Ok(ConversationSearchPage {
        total_count,
        hits: hits.into_iter().skip(offset).take(limit).collect(),
    })
}

pub(crate) async fn hydrate_conversation_search_matches_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    record_kind: ConversationRecordKind,
    adapter_id: Option<&str>,
    source_id: Option<&str>,
    query: &str,
    matches: crate::backend::domain::ConversationSearchMatches,
) -> StoreResult<ConversationSearchPage> {
    let tables = record_kind.tables();
    let adapter_card_kinds = load_search_adapter_card_kinds_sqlx(pool, tenant_id).await?;
    let session_ids = matches
        .hits
        .iter()
        .map(|matched| matched.session_id.as_str())
        .collect::<BTreeSet<_>>();
    let session_ids_json = serde_json::to_string(&session_ids).map_err(StoreError::external)?;
    let sessions = load_search_sessions_sqlx(
        pool,
        tenant_id,
        tables,
        adapter_id,
        source_id,
        Some(&session_ids_json),
    )
    .await?
    .into_iter()
    .map(|item| (item.session.id.clone(), item))
    .collect::<BTreeMap<_, _>>();
    let question_groups = load_search_questions_sqlx(
        pool,
        tenant_id,
        tables,
        adapter_id,
        source_id,
        Some(&session_ids_json),
    )
    .await?;
    let question_indices = question_groups
        .values()
        .flat_map(|questions| questions.iter().enumerate())
        .map(|(index, question)| (question.id.clone(), index as i64))
        .collect::<BTreeMap<_, _>>();
    let questions = question_groups
        .into_values()
        .flatten()
        .map(|item| (item.id.clone(), item))
        .collect::<BTreeMap<_, _>>();
    let turns_by_question = load_search_turns_sqlx(
        pool,
        tenant_id,
        tables,
        adapter_id,
        source_id,
        Some(&session_ids_json),
    )
    .await?;
    let turns = turns_by_question
        .values()
        .flatten()
        .cloned()
        .map(|item| (item.id.clone(), item))
        .collect::<BTreeMap<_, _>>();
    let parts = load_search_parts_sqlx(
        pool,
        tenant_id,
        tables,
        adapter_id,
        source_id,
        Some(&session_ids_json),
    )
    .await?
    .into_values()
    .flatten()
    .map(|item| (item.id.clone(), item))
    .collect::<BTreeMap<_, _>>();
    let needle = normalize_query(Some(query))
        .ok_or_else(|| StoreError::external("conversation search query is required".to_string()))?;
    let id_fragment = crate::backend::domain::conversation_id_search_term(query)
        .map(|value| crate::backend::domain::conversation_id_fragment(&value));
    let mut hits = Vec::with_capacity(matches.hits.len());

    for matched in matches.hits {
        let matched_by_id = id_fragment.as_deref().is_some_and(|fragment| {
            [
                matched.session_id.as_str(),
                matched.question_id.as_str(),
                matched.turn_id.as_str(),
                matched.part_id.as_str(),
                matched.document_id.as_str(),
            ]
            .into_iter()
            .any(|value| crate::backend::domain::conversation_id_fragment(value) == fragment)
        });
        let session = sessions.get(&matched.session_id).ok_or_else(|| {
            StoreError::external("conversation search index hydration missed a session".to_string())
        })?;
        let question = questions.get(&matched.question_id).ok_or_else(|| {
            StoreError::external(
                "conversation search index hydration missed a question".to_string(),
            )
        })?;
        let question_title = search_question_title_from_turns(
            question,
            turns_by_question
                .get(&question.id)
                .map(Vec::as_slice)
                .unwrap_or_default(),
        );
        let (part_id, text) = if matched.card_type == "question" {
            let turn = turns.get(&matched.turn_id).ok_or_else(|| {
                StoreError::external(
                    "conversation search index hydration missed a turn".to_string(),
                )
            })?;
            (None, turn.user_text.clone())
        } else {
            let part = parts.get(&matched.part_id).ok_or_else(|| {
                StoreError::external(
                    "conversation search index hydration missed a part".to_string(),
                )
            })?;
            let cards =
                crate::backend::domain::conversations::projection::project_conversation_content_cards(
                    part,
                    &session.session.adapter_id,
                    adapter_card_kinds
                        .get(&session.session.adapter_id)
                        .map(Vec::as_slice)
                        .unwrap_or_default(),
                )?;
            let card = cards
                .iter()
                .find(|card| card.node_id == matched.document_id)
                .ok_or_else(|| {
                    StoreError::external(
                        "conversation search index hydration missed a projected card".to_string(),
                    )
                })?;
            if card.kind != matched.card_type || card.part_id != part.id {
                return Err(StoreError::Validation(
                    "conversation search index hydration found stale card metadata".to_string(),
                ));
            }
            (Some(part.id.clone()), card.body.clone())
        };
        let card_type = content_card_type_value(&matched.card_type)
            .or_else(|| {
                (matched.card_type == "question").then_some(ConversationSearchCardType::question())
            })
            .ok_or_else(|| {
                StoreError::external(
                    "conversation search index returned an invalid card type".to_string(),
                )
            })?;
        hits.push(ConversationSearchHit {
            session: session.clone(),
            question_id: question.id.clone(),
            question_index: question_indices.get(&question.id).copied().unwrap_or(0),
            question_title,
            turn_id: Some(matched.turn_id),
            part_id,
            block_id: matched.document_id,
            card_type,
            snippet: if matched_by_id {
                leading_search_snippet(&text)
            } else {
                search_snippet(&text, &needle)
            },
            score: matched.score,
            incremental: None,
            highlight_segments: if matched_by_id {
                None
            } else {
                search_highlight_segments(&text, &needle)
            },
        });
    }
    Ok(ConversationSearchPage {
        total_count: matches.total_count,
        hits,
    })
}
