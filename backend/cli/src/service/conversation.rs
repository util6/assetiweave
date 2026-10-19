use anyhow::{Context, Result};
use asset_core::application::{
    AppService, ConversationAdapterPackageInspectParams, ConversationBlockGetParams,
    ConversationBlockListParams, ConversationDataAuditParams, ConversationDataRepairParams,
    ConversationDataRollbackParams, ConversationPartTranslationUpdateParams,
    ConversationQuestionGetParams, ConversationQuestionListParams, ConversationQuestionMergeParams,
    ConversationQuestionSplitParams, ConversationSyncMode, ConversationSyncParams,
};
use serde_json::Value;

pub struct ConversationOperations<'a> {
    app_service: &'a AppService,
}

impl<'a> ConversationOperations<'a> {
    pub fn new(app_service: &'a AppService) -> Self {
        Self { app_service }
    }

    pub async fn inspect_adapter(&self, adapter_id: String) -> Result<Value> {
        let params = ConversationAdapterPackageInspectParams {
            adapter_id: Some(adapter_id),
            package_id: None,
        };
        let result = self
            .app_service
            .inspect_conversation_adapter_package(params)
            .await
            .context("failed to inspect conversation adapter")?;
        Ok(serde_json::to_value(result)?)
    }

    pub async fn list_sources(&self) -> Result<Value> {
        let sources = self
            .app_service
            .list_sources()
            .await
            .context("failed to list sources")?;
        Ok(serde_json::to_value(sources)?)
    }

    pub async fn sync_source(&self, source_id: String, full: bool) -> Result<Value> {
        let params = ConversationSyncParams {
            source_id: Some(source_id),
            adapter_id: None,
            record_kind: None,
            mode: if full {
                ConversationSyncMode::Full
            } else {
                ConversationSyncMode::Incremental
            },
            dry_run: false,
        };
        let result = self
            .app_service
            .sync_conversations(params)
            .await
            .context("failed to sync conversation source")?;
        Ok(serde_json::to_value(result)?)
    }

    pub async fn audit_data(
        &self,
        source: Option<String>,
        record_kind: Option<String>,
    ) -> Result<Value> {
        let params = ConversationDataAuditParams {
            source_id: source,
            record_kind,
            include_resolved: false,
        };
        let result = self
            .app_service
            .audit_conversation_data(params)
            .await
            .context("failed to audit conversation data")?;
        Ok(serde_json::to_value(result)?)
    }

    pub async fn repair_data(
        &self,
        source: Option<String>,
        record_kind: Option<String>,
        dry_run: bool,
        resync: bool,
        yes: bool,
    ) -> Result<Value> {
        let params = ConversationDataRepairParams {
            source_id: source,
            record_kind,
            dry_run,
            yes,
            resync,
        };
        let result = self
            .app_service
            .repair_conversation_data(params)
            .await
            .context("failed to repair conversation data")?;
        Ok(serde_json::to_value(result)?)
    }

    pub async fn rollback_data(
        &self,
        backup_path: String,
        dry_run: bool,
        yes: bool,
    ) -> Result<Value> {
        let params = ConversationDataRollbackParams {
            backup_path,
            dry_run,
            yes,
        };
        let result = self
            .app_service
            .rollback_conversation_data(params)
            .await
            .context("failed to rollback conversation data")?;
        Ok(serde_json::to_value(result)?)
    }

    pub async fn list_questions(
        &self,
        session_id: String,
        limit: usize,
        offset: usize,
    ) -> Result<Value> {
        let params = ConversationQuestionListParams {
            session_id,
            query: None,
            limit: Some(limit),
            offset: Some(offset),
        };
        let result = self
            .app_service
            .list_conversation_questions(params)
            .await
            .context("failed to list conversation questions")?;
        Ok(serde_json::to_value(result)?)
    }

    pub async fn get_question(&self, question_id: String) -> Result<Value> {
        let params = ConversationQuestionGetParams { question_id };
        let result = self
            .app_service
            .get_conversation_question(params)
            .await
            .context("failed to get conversation question")?;
        Ok(serde_json::to_value(result)?)
    }

    pub async fn merge_question(&self, question_id: String, dry_run: bool) -> Result<Value> {
        let params = ConversationQuestionMergeParams {
            question_ids: vec![question_id],
            dry_run,
        };
        let result = self
            .app_service
            .merge_conversation_questions(params)
            .await
            .context("failed to merge conversation question")?;
        Ok(serde_json::to_value(result)?)
    }

    pub async fn split_question(
        &self,
        question_id: String,
        before_turn: String,
        dry_run: bool,
    ) -> Result<Value> {
        let params = ConversationQuestionSplitParams {
            question_id,
            before_turn_id: before_turn,
            dry_run,
        };
        let result = self
            .app_service
            .split_conversation_question(params)
            .await
            .context("failed to split conversation question")?;
        Ok(serde_json::to_value(result)?)
    }

    pub async fn list_blocks(&self, question_id: String) -> Result<Value> {
        let params = ConversationBlockListParams { question_id };
        let result = self
            .app_service
            .list_conversation_blocks(params)
            .await
            .context("failed to list conversation blocks")?;
        Ok(serde_json::to_value(result)?)
    }

    pub async fn get_block(&self, block_id: String) -> Result<Value> {
        let params = ConversationBlockGetParams { block_id };
        let result = self
            .app_service
            .get_conversation_block(params)
            .await
            .context("failed to get conversation block")?;
        Ok(serde_json::to_value(result)?)
    }

    pub async fn update_part_translation(
        &self,
        part_id: String,
        record_kind: String,
        text: String,
    ) -> Result<Value> {
        let params = ConversationPartTranslationUpdateParams {
            part_id,
            record_kind: Some(record_kind),
            translated_text: text,
        };
        let result = self
            .app_service
            .update_conversation_part_translation(params)
            .await
            .context("failed to update conversation part translation")?;
        Ok(serde_json::to_value(result)?)
    }

    pub async fn get_session_outline(&self, id: String) -> Result<Value> {
        let params = asset_core::application::ConversationSessionOutlineParams { id };
        let outline = self
            .app_service
            .get_conversation_session_outline(params)
            .await
            .context("failed to get conversation session outline")?;
        Ok(serde_json::to_value(outline)?)
    }
}
