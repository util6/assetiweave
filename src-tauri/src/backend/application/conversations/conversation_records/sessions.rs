use super::*;
use crate::backend::application::prelude::*;
use crate::backend::application::AppService;
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
