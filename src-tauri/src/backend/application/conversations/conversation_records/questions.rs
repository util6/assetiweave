use super::*;
use crate::backend::application::prelude::*;
use crate::backend::application::AppService;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
impl AppService {
    pub(crate) async fn list_conversation_questions(
        &self,
        params: ConversationQuestionListParams,
    ) -> AppResult<Vec<crate::backend::domain::conversations::ConversationQuestionDetail>> {
        let pool = self.pool();
        let tenant_id = self.tenant_id();
        let session_id = crate::backend::store::resolve_conversation_session_id_prefix_sqlx(
            pool,
            tenant_id,
            &params.session_id,
        )
        .await?;
        crate::backend::store::list_conversation_question_details_sqlx(
            pool,
            tenant_id,
            &session_id,
            params.query.as_deref(),
            params.limit.unwrap_or(100).clamp(1, 500),
            params.offset.unwrap_or(0),
        )
        .await
        .map_err(AppError::external)
    }

    pub(crate) async fn get_conversation_question(
        &self,
        params: ConversationQuestionGetParams,
    ) -> AppResult<crate::backend::domain::conversations::ConversationQuestionDetail> {
        let pool = self.pool();
        let tenant_id = self.tenant_id();
        let question_id = crate::backend::store::resolve_conversation_question_id_prefix_sqlx(
            pool,
            tenant_id,
            &params.question_id,
        )
        .await?;
        crate::backend::store::load_conversation_question_detail_sqlx(pool, tenant_id, &question_id)
            .await
            .map_err(AppError::external)
    }

    pub(crate) async fn list_conversation_blocks(
        &self,
        params: ConversationBlockListParams,
    ) -> AppResult<Vec<crate::backend::domain::conversations::ConversationBlockLocator>> {
        let record_kind = conversation_record_kind_from_locator(&params.question_id)?;
        crate::backend::store::list_conversation_block_locators_sqlx(
            self.pool(),
            self.tenant_id(),
            record_kind,
            &params.question_id,
        )
        .await
        .map_err(AppError::external)
    }

    pub(crate) async fn get_conversation_block(
        &self,
        params: ConversationBlockGetParams,
    ) -> AppResult<crate::backend::domain::conversations::ConversationBlockDetail> {
        let record_kind = conversation_record_kind_from_locator(&params.block_id)?;
        crate::backend::store::load_conversation_block_detail_sqlx(
            self.pool(),
            self.tenant_id(),
            record_kind,
            &params.block_id,
        )
        .await
        .map_err(AppError::external)
    }

    pub(crate) async fn merge_conversation_questions(
        &self,
        params: ConversationQuestionMergeParams,
    ) -> AppResult<crate::backend::domain::conversations::ConversationMutationResult> {
        let pool = self.pool();
        let tenant_id = self.tenant_id();
        let mut resolved_question_ids = Vec::with_capacity(params.question_ids.len());
        for q_id in &params.question_ids {
            resolved_question_ids.push(
                crate::backend::store::resolve_conversation_question_id_prefix_sqlx(
                    pool, tenant_id, q_id,
                )
                .await?,
            );
        }
        crate::backend::store::merge_conversation_questions_sqlx(
            pool,
            tenant_id,
            &resolved_question_ids,
            params.dry_run,
        )
        .await
        .map_err(AppError::external)
    }

    pub(crate) async fn split_conversation_question(
        &self,
        params: ConversationQuestionSplitParams,
    ) -> AppResult<crate::backend::domain::conversations::ConversationMutationResult> {
        let pool = self.pool();
        let tenant_id = self.tenant_id();
        let question_id = crate::backend::store::resolve_conversation_question_id_prefix_sqlx(
            pool,
            tenant_id,
            &params.question_id,
        )
        .await?;
        let before_turn_id = crate::backend::store::resolve_conversation_turn_id_prefix_sqlx(
            pool,
            tenant_id,
            &params.before_turn_id,
        )
        .await?;
        crate::backend::store::split_conversation_question_sqlx(
            pool,
            tenant_id,
            &question_id,
            &before_turn_id,
            params.dry_run,
        )
        .await
        .map_err(AppError::external)
    }

    pub(crate) async fn update_conversation_part_translation(
        &self,
        params: ConversationPartTranslationUpdateParams,
    ) -> AppResult<()> {
        let part_id = params.part_id.trim();
        if part_id.is_empty() {
            return Err(AppError::Validation(
                "conversation part id is required".to_string(),
            ));
        }
        if params.translated_text.len() > 200_000 {
            return Err(AppError::Validation(
                "conversation part translation is too large".to_string(),
            ));
        }

        let (_, record_kind) = normalize_conversation_record_kind(params.record_kind.as_deref())?;
        let pool = self.pool();
        let tenant_id = self.tenant_id();
        match record_kind {
            crate::backend::domain::conversations::ConversationRecordKind::Session => {
                let part_id = crate::backend::store::resolve_conversation_part_id_prefix_sqlx(
                    pool,
                    tenant_id,
                    &params.part_id,
                )
                .await
                .map_err(AppError::external)?;
                Ok(
                    crate::backend::store::update_conversation_part_translation_sqlx(
                        pool,
                        tenant_id,
                        &part_id,
                        &params.translated_text,
                    )
                    .await?,
                )
            }
            crate::backend::domain::conversations::ConversationRecordKind::Web => {
                let part_id = crate::backend::store::resolve_web_record_part_id_prefix_sqlx(
                    pool,
                    tenant_id,
                    &params.part_id,
                )
                .await?;
                Ok(
                    crate::backend::store::update_web_record_part_translation_sqlx(
                        pool,
                        tenant_id,
                        &part_id,
                        &params.translated_text,
                    )
                    .await?,
                )
            }
        }
    }
}
