use crate::backend::application::{
    AppError, AppResult, AppService, ConversationCardGetParams, ConversationOutlineParams,
};
use crate::backend::domain::conversations::outline::{
    CardOutline, CardTargetMeta, ConversationOutlineDetailLevel, MatrixView, NeighborhoodCardRow,
    SessionOutline, SessionTurnRow, TurnCardRow, TurnOutline,
};
use crate::backend::domain::conversations::{
    ConversationCardDetail, ConversationOutline, ConversationQuestionDetail,
    ConversationRecordKind, ConversationSessionDetail,
};
use crate::backend::domain::ConversationPartRole;
use crate::backend::store::conversations::outline::ResolvedOutlineTarget;
use sqlx::SqlitePool;

impl AppService {
    pub async fn get_conversation_outline(
        &self,
        params: ConversationOutlineParams,
    ) -> AppResult<ConversationOutline> {
        let pool = self.pool();
        let tenant_id = self.tenant_id();
        let target =
            crate::backend::store::resolve_outline_target_sqlx(pool, tenant_id, &params.id)
                .await
                .map_err(AppError::from)?;

        match target {
            ResolvedOutlineTarget::Session {
                record_kind,
                session_id,
            } => {
                let detail =
                    load_session_detail_by_kind(pool, tenant_id, record_kind, &session_id).await?;
                build_session_outline(detail, params.detail_level)
            }
            ResolvedOutlineTarget::Turn {
                record_kind,
                session_id,
                turn_id,
            } => {
                let detail =
                    load_session_detail_by_kind(pool, tenant_id, record_kind, &session_id).await?;
                build_turn_outline(detail, &turn_id)
            }
            ResolvedOutlineTarget::Card {
                record_kind,
                session_id,
                turn_id,
                card_id,
            } => {
                let detail =
                    load_session_detail_by_kind(pool, tenant_id, record_kind, &session_id).await?;
                build_card_outline(detail, &turn_id, &card_id)
            }
        }
    }

    pub async fn get_conversation_card(
        &self,
        params: ConversationCardGetParams,
    ) -> AppResult<ConversationCardDetail> {
        let block = self
            .get_conversation_block(crate::backend::application::ConversationBlockGetParams {
                block_id: params.card_id.clone(),
            })
            .await?;

        let non_empty = |frag: String, raw: String| if frag.is_empty() { raw } else { frag };
        let short_card_id =
            crate::backend::domain::conversation_id_fragment(&block.locator.block_id);
        let short_session_id =
            crate::backend::domain::conversation_id_fragment(&block.locator.session_id);
        let short_turn_id =
            crate::backend::domain::conversation_id_fragment(&block.locator.turn_id);

        Ok(ConversationCardDetail {
            card_id: non_empty(short_card_id, block.locator.block_id),
            session_id: non_empty(short_session_id, block.locator.session_id),
            turn_id: non_empty(short_turn_id, block.locator.turn_id),
            kind: block.locator.kind,
            label: block.locator.label,
            status: block.locator.status,
            content: block.content,
        })
    }
}

async fn load_session_detail_by_kind(
    pool: &SqlitePool,
    tenant_id: &str,
    record_kind: ConversationRecordKind,
    session_id: &str,
) -> AppResult<ConversationSessionDetail> {
    match record_kind {
        ConversationRecordKind::Session => {
            crate::backend::store::load_conversation_session_detail_sqlx(
                pool, tenant_id, session_id,
            )
            .await
            .map_err(AppError::external)
        }
        ConversationRecordKind::Web => {
            crate::backend::store::load_web_record_session_detail_sqlx(pool, tenant_id, session_id)
                .await
                .map_err(AppError::external)
        }
    }
}

#[derive(Clone)]
struct SimpleCardNode {
    node_id: String,
    part_id: String,
    node_type: String,
    command_label: Option<String>,
    status: Option<String>,
    role: ConversationPartRole,
    content_chars: usize,
}

fn collect_turn_cards(question: &ConversationQuestionDetail, turn_id: &str) -> Vec<SimpleCardNode> {
    let mut turn_nodes: Vec<_> = question
        .projected_content_nodes
        .iter()
        .filter(|node| node.turn_id == turn_id)
        .collect();
    turn_nodes.sort_by_key(|node| (node.part_order, node.node_order));

    let mut cards: Vec<SimpleCardNode> = turn_nodes
        .into_iter()
        .map(|node| SimpleCardNode {
            node_id: node.node_id.clone(),
            part_id: node.part_id.clone(),
            node_type: node.node_type.clone(),
            command_label: node.command_label.clone(),
            status: node.status.clone(),
            role: node.role,
            content_chars: node.content.chars().count(),
        })
        .collect();

    let mut unprojected_parts: Vec<_> = question
        .parts
        .iter()
        .filter(|p| p.turn_id == turn_id && p.role != ConversationPartRole::User)
        .filter(|p| !cards.iter().any(|c| c.part_id == p.id))
        .collect();
    unprojected_parts.sort_by_key(|p| p.part_index);

    for part in unprojected_parts {
        let chars = part
            .text
            .as_deref()
            .or(part.command.as_deref())
            .unwrap_or_default()
            .chars()
            .count();
        cards.push(SimpleCardNode {
            node_id: part.id.clone(),
            part_id: part.id.clone(),
            node_type: part.kind.as_str().to_string(),
            command_label: part.command_label.clone(),
            status: part.status.clone(),
            role: part.role,
            content_chars: chars,
        });
    }

    cards
}

fn build_session_outline(
    detail: ConversationSessionDetail,
    detail_level: ConversationOutlineDetailLevel,
) -> AppResult<ConversationOutline> {
    let session_short_id = crate::backend::domain::conversation_id_fragment(&detail.session.id);
    let mut all_turns = Vec::new();
    let need_cards_and_activities = matches!(
        detail_level,
        ConversationOutlineDetailLevel::Default | ConversationOutlineDetailLevel::Full
    );

    for question in &detail.questions {
        for turn in &question.turns {
            let (activities, cards_count) = if need_cards_and_activities {
                let cards = collect_turn_cards(question, &turn.id);
                let mut acts = Vec::new();
                for card in &cards {
                    if let Some(ref label) = card.command_label {
                        let trimmed = label.trim();
                        if !trimmed.is_empty()
                            && acts.last().map(|s: &String| s.as_str()) != Some(trimmed)
                        {
                            acts.push(trimmed.to_string());
                        }
                    }
                }
                if acts.len() > 15 {
                    acts.truncate(15);
                }
                (acts, cards.len())
            } else {
                (Vec::new(), 0)
            };

            let question_text = if !turn.prompt.trim().is_empty() {
                turn.prompt.clone()
            } else if let Some(ref title) = question.question.title {
                title.clone()
            } else {
                "Question".to_string()
            };

            all_turns.push((
                turn.turn_index,
                turn.id.clone(),
                question_text,
                activities,
                cards_count,
                turn.response.clone(),
            ));
        }
    }

    all_turns.sort_by_key(|t| t.0);
    let total_turns = all_turns.len();

    let (columns, rows) = match detail_level {
        ConversationOutlineDetailLevel::Questions => (
            vec!["index".into(), "turn_id".into(), "question".into()],
            all_turns
                .into_iter()
                .enumerate()
                .map(
                    |(i, (_, turn_id, question, _, _, _))| SessionTurnRow::Questions {
                        index: i + 1,
                        turn_id: crate::backend::domain::conversation_id_fragment(&turn_id),
                        question,
                    },
                )
                .collect(),
        ),
        ConversationOutlineDetailLevel::Qa => (
            vec![
                "index".into(),
                "turn_id".into(),
                "question".into(),
                "answer".into(),
            ],
            all_turns
                .into_iter()
                .enumerate()
                .map(
                    |(i, (_, turn_id, question, _, _, answer))| SessionTurnRow::Qa {
                        index: i + 1,
                        turn_id: crate::backend::domain::conversation_id_fragment(&turn_id),
                        question,
                        answer,
                    },
                )
                .collect(),
        ),
        ConversationOutlineDetailLevel::Full => (
            vec![
                "index".into(),
                "turn_id".into(),
                "question".into(),
                "activities".into(),
                "cards".into(),
                "answer".into(),
            ],
            all_turns
                .into_iter()
                .enumerate()
                .map(|(i, (_, turn_id, question, activities, cards, answer))| {
                    SessionTurnRow::Full {
                        index: i + 1,
                        turn_id: crate::backend::domain::conversation_id_fragment(&turn_id),
                        question,
                        activities,
                        cards,
                        answer,
                    }
                })
                .collect(),
        ),
        ConversationOutlineDetailLevel::Default => (
            vec![
                "index".into(),
                "turn_id".into(),
                "question".into(),
                "activities".into(),
                "cards".into(),
            ],
            all_turns
                .into_iter()
                .enumerate()
                .map(
                    |(i, (_, turn_id, question, activities, cards, _))| SessionTurnRow::Default {
                        index: i + 1,
                        turn_id: crate::backend::domain::conversation_id_fragment(&turn_id),
                        question,
                        activities,
                        cards,
                    },
                )
                .collect(),
        ),
    };

    Ok(ConversationOutline::Session(SessionOutline {
        session_id: session_short_id,
        source_id: detail.session.source_id.clone(),
        total_turns,
        created_at: detail.session.started_at.clone(),
        turns: MatrixView { columns, rows },
    }))
}

fn build_turn_outline(
    detail: ConversationSessionDetail,
    target_turn_id: &str,
) -> AppResult<ConversationOutline> {
    let session_short_id = crate::backend::domain::conversation_id_fragment(&detail.session.id);

    let mut turns_with_questions = Vec::new();
    for question in &detail.questions {
        for turn in &question.turns {
            turns_with_questions.push((turn.clone(), question));
        }
    }
    turns_with_questions.sort_by_key(|(turn, _)| turn.turn_index);
    let total_turns = turns_with_questions.len();

    let clean_target = target_turn_id.trim();
    let target_idx = turns_with_questions.iter().position(|(t, _)| {
        t.id == clean_target
            || crate::backend::domain::conversation_id_fragment(&t.id) == clean_target
    });

    let target_pos = target_idx.ok_or_else(|| {
        AppError::NotFound(format!("turn not found in session: {target_turn_id}"))
    })?;

    let (turn, question) = &turns_with_questions[target_pos];
    let turn_short_id = crate::backend::domain::conversation_id_fragment(&turn.id);
    let cards = collect_turn_cards(question, &turn.id);

    let rows: Vec<TurnCardRow> = cards
        .into_iter()
        .enumerate()
        .map(|(idx, card)| TurnCardRow {
            order: idx + 1,
            card_id: crate::backend::domain::conversation_id_fragment(&card.node_id),
            kind: card.node_type,
            label: card.command_label,
            status: card.status,
            chars: card.content_chars,
        })
        .collect();

    let user_question = if !turn.prompt.trim().is_empty() {
        turn.prompt.clone()
    } else if let Some(ref title) = question.question.title {
        title.clone()
    } else {
        "Question".to_string()
    };

    Ok(ConversationOutline::Turn(TurnOutline {
        session_id: session_short_id,
        turn_id: turn_short_id,
        turn_index: target_pos + 1,
        total_turns,
        user_question,
        cards: MatrixView {
            columns: vec![
                "order".into(),
                "card_id".into(),
                "kind".into(),
                "label".into(),
                "status".into(),
                "chars".into(),
            ],
            rows,
        },
    }))
}

fn build_card_outline(
    detail: ConversationSessionDetail,
    _target_turn_id: &str,
    target_card_id: &str,
) -> AppResult<ConversationOutline> {
    let session_short_id = crate::backend::domain::conversation_id_fragment(&detail.session.id);
    let clean_card_id = target_card_id.trim();

    let mut turns_with_cards = Vec::new();
    for question in &detail.questions {
        for turn in &question.turns {
            let cards = collect_turn_cards(question, &turn.id);
            turns_with_cards.push((turn.clone(), question.clone(), cards));
        }
    }
    turns_with_cards.sort_by_key(|(t, _, _)| t.turn_index);

    let mut found_location = None;
    'outer: for (turn_idx, (turn, question, cards)) in turns_with_cards.iter().enumerate() {
        for (card_idx, card) in cards.iter().enumerate() {
            let matches = card.node_id == clean_card_id
                || card.part_id == clean_card_id
                || crate::backend::domain::conversation_id_fragment(&card.node_id) == clean_card_id
                || crate::backend::domain::conversation_id_fragment(&card.part_id) == clean_card_id;
            if matches {
                found_location = Some((
                    turn_idx,
                    card_idx,
                    turn.clone(),
                    question.clone(),
                    cards.clone(),
                ));
                break 'outer;
            }
        }
    }

    let (turn_pos, card_pos, turn, question, cards) = found_location.ok_or_else(|| {
        AppError::NotFound(format!("card not found in session: {target_card_id}"))
    })?;

    let target_node = &cards[card_pos];
    let target_meta = CardTargetMeta {
        card_id: crate::backend::domain::conversation_id_fragment(&target_node.node_id),
        order: card_pos + 1,
        kind: target_node.node_type.clone(),
        label: target_node.command_label.clone(),
        status: target_node.status.clone(),
        chars: target_node.content_chars,
    };

    let start_idx = card_pos.saturating_sub(1);
    let end_idx = (card_pos + 2).min(cards.len());

    let neighbor_rows: Vec<NeighborhoodCardRow> = cards[start_idx..end_idx]
        .iter()
        .enumerate()
        .map(|(offset, node)| {
            let actual_order = start_idx + offset + 1;
            let is_target = actual_order == card_pos + 1;
            NeighborhoodCardRow {
                order: actual_order,
                card_id: crate::backend::domain::conversation_id_fragment(&node.node_id),
                kind: node.node_type.clone(),
                label: node.command_label.clone(),
                is_target,
            }
        })
        .collect();

    let user_question = if !turn.prompt.trim().is_empty() {
        turn.prompt.clone()
    } else if let Some(ref title) = question.question.title {
        title.clone()
    } else {
        "Question".to_string()
    };

    Ok(ConversationOutline::Card(CardOutline {
        session_id: session_short_id,
        turn_id: crate::backend::domain::conversation_id_fragment(&turn.id),
        turn_index: turn_pos + 1,
        turn_user_question: user_question,
        target: target_meta,
        neighborhood: MatrixView {
            columns: vec![
                "order".into(),
                "card_id".into(),
                "kind".into(),
                "label".into(),
                "is_target".into(),
            ],
            rows: neighbor_rows,
        },
    }))
}

#[cfg(test)]
#[path = "outline_tests.rs"]
mod tests;
