use super::*;
use crate::backend::application::prelude::*;
use crate::backend::application::AppService;
use crate::backend::domain::conversations::{ConversationOutline, ConversationRecordKind};
use crate::backend::domain::{ConversationPartKind, ConversationPartRole};
use chrono::{DateTime, Duration, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
impl AppService {
    pub async fn list_conversation_sessions(
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
            let (since, until) = resolve_time_window_bounds(
                params.window.as_deref(),
                params.since.as_deref(),
                params.until.as_deref(),
                params.range.as_deref(),
            )?;
            let filter = crate::backend::store::ConversationSessionListFilter {
                adapter_id: adapter_id.as_deref(),
                source_id: source_id.as_deref(),
                query: query.as_deref(),
                project_path: params.project_path.as_deref(),
                since: since.as_deref(),
                until: until.as_deref(),
                limit,
                offset,
            };
            Ok(
                crate::backend::store::list_conversation_sessions_filtered_sqlx(
                    pool, tenant_id, filter,
                )
                .await?,
            )
        }
    }

    pub async fn get_conversation_session(
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
        let mut detail = crate::backend::store::load_conversation_session_detail_sqlx(
            pool,
            tenant_id,
            &session_id,
        )
        .await
        .map_err(AppError::external)?;

        if let Some(raw_roles) = params.roles {
            let role_set: std::collections::HashSet<String> = raw_roles
                .into_iter()
                .flat_map(|r| {
                    r.split(',')
                        .map(|s| s.trim().to_lowercase())
                        .collect::<Vec<_>>()
                })
                .filter(|s| !s.is_empty())
                .collect();

            if !role_set.is_empty() {
                let keep_questions = role_set.contains("question") || role_set.contains("user");
                for q in &mut detail.questions {
                    if !keep_questions {
                        for turn in &mut q.turns {
                            turn.prompt.clear();
                        }
                    }

                    q.projected_content_nodes.retain(|node| {
                        let kind = node.node_type.to_lowercase();
                        let semantic = node
                            .semantic_role
                            .as_deref()
                            .map(|s| s.to_lowercase())
                            .unwrap_or_default();

                        role_set.contains(&kind)
                            || (!semantic.is_empty() && role_set.contains(&semantic))
                            || (kind == "text" && role_set.contains("answer"))
                    });

                    q.parts.retain(|part| {
                        let kind = part.kind.as_str().to_lowercase();
                        let role = format!("{:?}", part.role).to_lowercase();

                        role_set.contains(&kind)
                            || role_set.contains(&role)
                            || (part.role == ConversationPartRole::Assistant
                                && part.kind == ConversationPartKind::Text
                                && role_set.contains("answer"))
                            || (part.role == ConversationPartRole::User
                                && (role_set.contains("question") || role_set.contains("user")))
                    });
                }
            }
        }

        Ok(detail)
    }

    pub async fn get_conversation_session_outline(
        &self,
        params: ConversationSessionOutlineParams,
    ) -> AppResult<ConversationOutline> {
        self.get_conversation_outline(crate::backend::application::ConversationOutlineParams {
            id: params.id,
        })
        .await
    }

    pub async fn list_web_record_sessions(
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

    pub async fn get_web_record_session(
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

    pub async fn search_conversation_records(
        &self,
        params: ConversationSearchParams,
    ) -> AppResult<ConversationSearchResult> {
        self.search_conversation_records_with_recent_deltas(params, None)
            .await
    }

    pub async fn replay_conversation_session_projection(
        &self,
        session_id: &str,
    ) -> AppResult<crate::backend::domain::conversations::ConversationSessionDetail> {
        let pool = self.pool();
        let tenant_id = self.tenant_id();
        let resolved_id = crate::backend::store::resolve_conversation_session_id_prefix_sqlx(
            pool, tenant_id, session_id,
        )
        .await?;

        let detail = crate::backend::store::load_conversation_session_detail_sqlx(
            pool,
            tenant_id,
            &resolved_id,
        )
        .await
        .map_err(AppError::external)?;

        let source = crate::backend::store::load_conversation_source_sqlx(
            pool,
            tenant_id,
            &detail.session.source_id,
        )
        .await?
        .ok_or_else(|| {
            AppError::NotFound(format!(
                "conversation source '{}' not found",
                detail.session.source_id
            ))
        })?;

        let adapter = crate::backend::store::load_conversation_adapter_sqlx(
            pool,
            tenant_id,
            &source.adapter_id,
        )
        .await?;

        let adapter_ref = adapter.as_ref().ok_or_else(|| {
            AppError::NotFound(format!(
                "conversation adapter '{}' not found for source '{}'",
                source.adapter_id, source.id
            ))
        })?;

        let settings = self.app_settings_value();
        let read_result =
            crate::backend::infrastructure::conversations::run_external_adapter_read_session(
                adapter_ref,
                &source,
                Some(&detail.session.external_id),
                &settings,
            )
            .await
            .map_err(|e| AppError::External(e.to_string()))?;

        let normalized = read_result
            .sessions
            .into_iter()
            .find(|s| s.external_id == detail.session.external_id)
            .ok_or_else(|| {
                AppError::NotFound(format!(
                    "session '{}' not found in adapter source output",
                    detail.session.external_id
                ))
            })?;

        let adapter_content_hash = adapter_ref.content_hash.clone();
        let card_contract_version = adapter_ref.card_contract_version;
        let payload_policy_version =
            crate::backend::infrastructure::conversations::CONVERSATION_PAYLOAD_POLICY_VERSION;
        let projection_version = adapter_ref.projection_version;

        crate::backend::store::reproject_conversation_session_sqlx(
            pool,
            tenant_id,
            &source,
            &normalized,
            adapter_content_hash.as_deref(),
            card_contract_version,
            payload_policy_version,
            projection_version,
        )
        .await?;

        crate::backend::store::load_conversation_session_detail_sqlx(pool, tenant_id, &resolved_id)
            .await
            .map_err(AppError::external)
    }
}

#[cfg(test)]
#[path = "sessions_tests.rs"]
mod tests;
