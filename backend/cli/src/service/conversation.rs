use anyhow::{Context, Result};
use asset_core::application::{
    AppService, ConversationAdapterPackageInspectParams, ConversationCardGetParams,
    ConversationDataAuditParams, ConversationDataRepairParams, ConversationDataRollbackParams,
    ConversationOutlineParams, ConversationSyncMode, ConversationSyncParams,
};
use asset_core::domain::conversations::ConversationCardDetail;
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

    pub async fn sync_conversations(
        &self,
        adapter_id: Option<String>,
        source_id: Option<String>,
        full: bool,
        dry_run: bool,
        record_kind: Option<String>,
    ) -> Result<Value> {
        let mode = if full {
            ConversationSyncMode::Full
        } else {
            ConversationSyncMode::Incremental
        };
        let params = ConversationSyncParams {
            source_id,
            adapter_id,
            record_kind,
            mode,
            dry_run,
        };
        let result = self
            .app_service
            .sync_conversations(params)
            .await
            .context("failed to synchronize conversations")?;
        Ok(serde_json::to_value(result)?)
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

    pub async fn get_outline(&self, id: String) -> Result<Value> {
        let params = ConversationOutlineParams { id };
        let outline = self
            .app_service
            .get_conversation_outline(params)
            .await
            .context("failed to get conversation outline")?;
        Ok(serde_json::to_value(outline)?)
    }

    pub async fn get_card(&self, card_id: String) -> Result<ConversationCardDetail> {
        let params = ConversationCardGetParams { card_id };
        let card = self
            .app_service
            .get_conversation_card(params)
            .await
            .context("failed to get conversation card")?;
        Ok(card)
    }
}
