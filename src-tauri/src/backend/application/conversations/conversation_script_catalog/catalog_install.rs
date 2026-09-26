use super::*;
use crate::backend::application::prelude::*;
use crate::backend::domain::{
    ConversationAdapterPackageChangeAction, ConversationAdapterPackageChangeRisk,
    ConversationAdapterPackageOrigin, ConversationAdapterPackageRecordKind,
    ConversationAdapterRuntimeGateStatus, ConversationPackageUpdatePolicy,
};
use crate::backend::infrastructure::conversations::{
    ConversationAdapterPackageInstallSourceKind, ConversationAdapterPackageInstallSpec,
};

impl AppService {
    pub(crate) async fn install_conversation_adapter_package(
        &self,
        params: ConversationAdapterPackageInstallParams,
    ) -> AppResult<Value> {
        let preflight = self
            .prepare_conversation_adapter_package_change(ConversationAdapterPackageChangeParams {
                action: ConversationAdapterPackageChangeAction::Install,
                package_id: Some(params.package_id.clone()),
                adapter_id: None,
            })
            .await?;
        reject_conversation_package_task_conflicts(&preflight)?;
        if !params.dry_run && !params.yes {
            return Err(AppError::Validation(
                "conversation adapter package install requires --yes".to_string(),
            ));
        }
        if params
            .version
            .as_deref()
            .and_then(clean_non_empty_string)
            .is_some()
        {
            let result = self
                .install_conversation_adapter_package_release(params)
                .await?;
            self.runtime.refresh_conversation_adapter_catalog().await?;
            return Ok(result);
        }

        let catalog = load_conversation_script_catalog(params.catalog_url.as_deref())?;
        let package_id = params.package_id.trim();
        let item = catalog
            .items
            .into_iter()
            .find(|item| item.package_id() == package_id)
            .ok_or_else(|| format!("conversation adapter package not found: {package_id}"))
            .map_err(AppError::external)?;
        validate_conversation_script_catalog_item(&item)?;

        let result = install_conversation_adapter_package_from_item(
            self,
            &item,
            params.dry_run,
            params.catalog_url.as_deref(),
        )
        .await?;
        if !params.dry_run {
            self.runtime.refresh_conversation_adapter_catalog().await?;
        }
        Ok(result)
    }

    pub(crate) async fn update_conversation_adapter_package(
        &self,
        params: ConversationAdapterPackageInstallParams,
    ) -> AppResult<Value> {
        let preflight = self
            .prepare_conversation_adapter_package_change(ConversationAdapterPackageChangeParams {
                action: ConversationAdapterPackageChangeAction::Update,
                package_id: Some(params.package_id.clone()),
                adapter_id: None,
            })
            .await?;
        reject_conversation_package_task_conflicts(&preflight)?;
        if !params.dry_run && !params.yes {
            return Err(AppError::Validation(
                "conversation adapter package update requires --yes".to_string(),
            ));
        }
        if params
            .version
            .as_deref()
            .and_then(clean_non_empty_string)
            .is_some()
        {
            let result = self
                .install_conversation_adapter_package_release(params)
                .await?;
            self.runtime.refresh_conversation_adapter_catalog().await?;
            return Ok(result);
        }

        let catalog = load_conversation_script_catalog(params.catalog_url.as_deref())?;
        let package_id = params.package_id.trim();
        let item = catalog
            .items
            .into_iter()
            .find(|item| item.package_id() == package_id)
            .ok_or_else(|| format!("conversation adapter package not found: {package_id}"))
            .map_err(AppError::external)?;
        validate_conversation_script_catalog_item(&item)?;
        let result = install_conversation_adapter_package_from_item(
            self,
            &item,
            params.dry_run,
            params.catalog_url.as_deref(),
        )
        .await?;
        if !params.dry_run {
            self.runtime.refresh_conversation_adapter_catalog().await?;
        }
        Ok(result)
    }

    pub(crate) async fn uninstall_conversation_adapter_package(
        &self,
        params: ConversationAdapterPackageUninstallParams,
    ) -> AppResult<Value> {
        let preflight = self
            .prepare_conversation_adapter_package_change(ConversationAdapterPackageChangeParams {
                action: ConversationAdapterPackageChangeAction::Uninstall,
                package_id: Some(params.package_id.clone()),
                adapter_id: None,
            })
            .await?;
        reject_conversation_package_task_conflicts(&preflight)?;
        if !params.dry_run && !params.yes {
            return Err(AppError::Validation(
                "conversation adapter package uninstall requires --yes".to_string(),
            ));
        }
        let package_id = params.package_id.trim();
        if package_id.is_empty() {
            return Err(AppError::Validation(
                "conversation adapter package id is required".to_string(),
            ));
        }
        let package = self
            .load_conversation_adapter_package(package_id)
            .await?
            .ok_or_else(|| format!("conversation adapter package not found: {package_id}"))
            .map_err(AppError::external)?;

        if params.dry_run {
            return Ok(json!({
                "dry_run": true,
                "uninstalled": false,
                "package": package,
                "preflight": preflight
            }));
        }

        let uninstalled = super::super::conversation_storage::deactivate_package(
            self.db.pool(),
            &package.package_id,
            &package.adapter_id,
        )
        .await
        .map_err(AppError::external)?;
        self.runtime.refresh_conversation_adapter_catalog().await?;
        Ok(json!({
            "dry_run": false,
            "uninstalled": true,
            "package": uninstalled,
            "preserved_managed_paths": preflight.managed_paths
        }))
    }

    pub(crate) async fn install_conversation_script(
        &self,
        params: ConversationScriptInstallParams,
    ) -> AppResult<Value> {
        self.install_conversation_adapter_package(ConversationAdapterPackageInstallParams {
            catalog_url: params.catalog_url,
            package_id: params.item_id,
            version: None,
            dry_run: params.dry_run,
            yes: params.yes,
        })
        .await
    }
}
