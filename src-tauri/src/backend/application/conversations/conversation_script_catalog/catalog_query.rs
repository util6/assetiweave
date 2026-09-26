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
    pub(crate) async fn list_conversation_adapter_packages(
        &self,
        params: ConversationAdapterPackageCatalogParams,
    ) -> AppResult<Vec<ConversationAdapterPackageCatalogEntry>> {
        let mut catalog = load_conversation_script_catalog(params.catalog_url.as_deref())?;
        for item in discover_local_conversation_adapter_packages(
            &crate::backend::infrastructure::app_settings::conversation_adapter_dir()?,
        )? {
            if let Some(existing) = catalog
                .items
                .iter_mut()
                .find(|existing| existing.package_id() == item.package_id())
            {
                *existing = item;
            } else {
                catalog.items.push(item);
            }
        }
        let adapters = self.list_conversation_adapters()?;
        let mut packages = self.load_conversation_adapter_packages().await?;
        for package in &mut packages {
            let adapter = adapters
                .iter()
                .find(|adapter| adapter.id == package.adapter_id);
            self.refresh_conversation_adapter_package_runtime(package, adapter)
                .await?;
        }
        Ok(resolve_conversation_adapter_package_catalog_entries(
            catalog.items,
            &adapters,
            &packages,
        ))
    }

    pub(crate) async fn list_conversation_script_catalog(
        &self,
        params: ConversationScriptCatalogParams,
    ) -> AppResult<Vec<ConversationScriptCatalogEntry>> {
        let entries = self
            .list_conversation_adapter_packages(ConversationAdapterPackageCatalogParams {
                catalog_url: params.catalog_url,
            })
            .await?;
        Ok(entries
            .into_iter()
            .map(ConversationScriptCatalogEntry::from)
            .collect())
    }

    pub(crate) async fn load_conversation_adapter_packages(
        &self,
    ) -> AppResult<Vec<ConversationAdapterPackage>> {
        super::super::conversation_storage::list_packages(self.db.pool()).await
    }

    pub(crate) async fn load_conversation_adapter_package(
        &self,
        package_id: &str,
    ) -> AppResult<Option<ConversationAdapterPackage>> {
        super::super::conversation_storage::load_package(self.db.pool(), package_id).await
    }

    pub(crate) async fn load_conversation_adapter_package_versions(
        &self,
        package_id: &str,
    ) -> AppResult<Vec<crate::backend::domain::ConversationAdapterPackageVersion>> {
        super::super::conversation_storage::list_package_versions(self.db.pool(), package_id).await
    }

    pub(crate) async fn list_installed_conversation_adapter_package_versions(
        &self,
        params: ConversationAdapterPackageVersionChangeParams,
    ) -> AppResult<Vec<crate::backend::domain::ConversationAdapterPackageVersion>> {
        self.load_conversation_adapter_package_versions(params.package_id.trim())
            .await
    }

    pub(crate) async fn load_conversation_adapter_package_by_adapter(
        &self,
        adapter_id: &str,
    ) -> AppResult<Option<ConversationAdapterPackage>> {
        super::super::conversation_storage::load_package_by_adapter(self.db.pool(), adapter_id)
            .await
    }

    pub(crate) async fn save_conversation_adapter_package(
        &self,
        package: &ConversationAdapterPackage,
    ) -> AppResult<()> {
        super::super::conversation_storage::save_package(self.db.pool(), package).await
    }
}
