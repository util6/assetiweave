use super::*;
use crate::backend::application::prelude::*;
use crate::backend::application::AppService;
use crate::backend::domain::conversations::*;
use crate::backend::domain::*;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
impl AppService {
    pub(crate) async fn list_conversation_sessions(
        &self,
        params: ConversationSessionListParams,
    ) -> AppResult<Vec<crate::backend::domain::conversations::ConversationSessionListItem>> {
        let pool = self.pool();
        let tenant_id = self.tenant_id();
        let adapter_id = params.adapter_id;
        let source_id = params.source_id;
        let query = params.query;
        let limit = params.limit.unwrap_or(50).clamp(1, 500);
        let offset = params.offset.unwrap_or(0);
        let direct_id_query = query.as_deref().is_some_and(|value| {
            value.trim().len() == 8
                && crate::backend::domain::conversation_id_search_term(value).is_some()
        });
        if direct_id_query {
            Ok(
                crate::backend::store::list_conversation_sessions_by_id_fragment_sqlx(
                    pool,
                    tenant_id,
                    crate::backend::domain::conversations::ConversationRecordKind::Session,
                    adapter_id.as_deref(),
                    source_id.as_deref(),
                    query.as_deref().unwrap_or_default(),
                    limit,
                    offset,
                )
                .await?,
            )
        } else {
            Ok(crate::backend::store::list_conversation_sessions_sqlx(
                pool,
                tenant_id,
                adapter_id.as_deref(),
                source_id.as_deref(),
                query.as_deref(),
                limit,
                offset,
            )
            .await?)
        }
    }

    pub(crate) async fn get_conversation_session(
        &self,
        params: ConversationSessionGetParams,
    ) -> AppResult<crate::backend::domain::conversations::ConversationSessionDetail> {
        let pool = self.pool();
        let tenant_id = self.tenant_id();
        let session_id = crate::backend::store::resolve_conversation_session_id_prefix_sqlx(
            pool,
            tenant_id,
            &params.session_id,
        )
        .await?;
        crate::backend::store::load_conversation_session_detail_sqlx(pool, tenant_id, &session_id)
            .await
            .map_err(AppError::external)
    }

    pub(crate) async fn get_conversation_session_outline(
        &self,
        params: ConversationSessionOutlineParams,
    ) -> AppResult<ConversationSessionOutline> {
        let pool = self.pool();
        let tenant_id = self.tenant_id();
        let (record_kind, resolved_session_id) =
            crate::backend::store::resolve_any_id_to_session_id_sqlx(pool, tenant_id, &params.id)
                .await?;

        let detail = match record_kind {
            ConversationRecordKind::Session => {
                crate::backend::store::load_conversation_session_detail_sqlx(
                    pool,
                    tenant_id,
                    &resolved_session_id,
                )
                .await
                .map_err(AppError::external)?
            }
            ConversationRecordKind::Web => {
                crate::backend::store::load_web_record_session_detail_sqlx(
                    pool,
                    tenant_id,
                    &resolved_session_id,
                )
                .await
                .map_err(AppError::external)?
            }
        };

        let mut turns = Vec::new();
        for question in &detail.questions {
            for turn in &question.turns {
                let mut turn_nodes: Vec<_> = question
                    .projected_content_nodes
                    .iter()
                    .filter(|node| node.turn_id == turn.id)
                    .collect();
                turn_nodes.sort_by_key(|node| (node.part_order, node.node_order));

                let mut cards = Vec::<ConversationCardRunGroup>::new();
                if !turn_nodes.is_empty() {
                    for node in turn_nodes {
                        let card_id =
                            crate::backend::domain::conversation_id_fragment(&node.node_id);
                        let kind = node.node_type.clone();
                        if let Some(last) = cards.last_mut() {
                            if last.card_kind == kind {
                                last.count += 1;
                                last.card_ids.push(card_id);
                                continue;
                            }
                        }
                        cards.push(ConversationCardRunGroup {
                            card_kind: kind,
                            count: 1,
                            card_ids: vec![card_id],
                        });
                    }
                } else {
                    let mut turn_parts: Vec<_> = question
                        .parts
                        .iter()
                        .filter(|part| {
                            part.turn_id == turn.id && part.role != ConversationPartRole::User
                        })
                        .collect();
                    turn_parts.sort_by_key(|part| part.part_index);
                    for part in turn_parts {
                        let card_id = crate::backend::domain::conversation_id_fragment(&part.id);
                        let kind = match (part.role, part.kind) {
                            (ConversationPartRole::Assistant, ConversationPartKind::Text) => {
                                "answer".to_string()
                            }
                            _ => part.kind.as_str().to_string(),
                        };
                        if let Some(last) = cards.last_mut() {
                            if last.card_kind == kind {
                                last.count += 1;
                                last.card_ids.push(card_id);
                                continue;
                            }
                        }
                        cards.push(ConversationCardRunGroup {
                            card_kind: kind,
                            count: 1,
                            card_ids: vec![card_id],
                        });
                    }
                }

                turns.push(ConversationTurnOutline {
                    turn_id: crate::backend::domain::conversation_id_fragment(&turn.id),
                    turn_index: turn.turn_index.max(0) as usize,
                    user_question: turn.user_text.clone(),
                    cards,
                });
            }
        }

        turns.sort_by_key(|turn| turn.turn_index);

        Ok(ConversationSessionOutline {
            session_id: crate::backend::domain::conversation_id_fragment(&detail.session.id),
            title: (!detail.session.title.trim().is_empty()).then(|| detail.session.title.clone()),
            turns,
        })
    }

    pub(crate) async fn list_web_record_sessions(
        &self,
        params: ConversationSessionListParams,
    ) -> AppResult<Vec<crate::backend::domain::conversations::ConversationSessionListItem>> {
        let pool = self.pool();
        let tenant_id = self.tenant_id();
        let adapter_id = params.adapter_id;
        let source_id = params.source_id;
        let query = params.query;
        let limit = params.limit.unwrap_or(50).clamp(1, 500);
        let offset = params.offset.unwrap_or(0);
        let direct_id_query = query.as_deref().is_some_and(|value| {
            value.trim().len() == 8
                && crate::backend::domain::conversation_id_search_term(value).is_some()
        });
        if direct_id_query {
            Ok(
                crate::backend::store::list_conversation_sessions_by_id_fragment_sqlx(
                    pool,
                    tenant_id,
                    crate::backend::domain::conversations::ConversationRecordKind::Web,
                    adapter_id.as_deref(),
                    source_id.as_deref(),
                    query.as_deref().unwrap_or_default(),
                    limit,
                    offset,
                )
                .await?,
            )
        } else {
            Ok(crate::backend::store::list_web_record_sessions_sqlx(
                pool,
                tenant_id,
                adapter_id.as_deref(),
                source_id.as_deref(),
                query.as_deref(),
                limit,
                offset,
            )
            .await?)
        }
    }

    pub(crate) async fn get_web_record_session(
        &self,
        params: ConversationSessionGetParams,
    ) -> AppResult<crate::backend::domain::conversations::ConversationSessionDetail> {
        let pool = self.pool();
        let tenant_id = self.tenant_id();
        let session_id = crate::backend::store::resolve_web_record_session_id_prefix_sqlx(
            pool,
            tenant_id,
            &params.session_id,
        )
        .await?;
        Ok(
            crate::backend::store::load_web_record_session_detail_sqlx(
                pool,
                tenant_id,
                &session_id,
            )
            .await?,
        )
    }

    pub(crate) async fn search_conversation_records(
        &self,
        params: ConversationSearchParams,
    ) -> AppResult<ConversationSearchResult> {
        self.search_conversation_records_with_recent_deltas(params, None)
            .await
    }
}

#[cfg(test)]
#[path = "sessions_tests.rs"]
mod tests;
