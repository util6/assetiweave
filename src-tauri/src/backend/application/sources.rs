use super::prelude::*;
use crate::backend::runtime::tasks::TaskContext;
use crate::backend::runtime::{AppError, AppResult};

#[derive(Debug, Clone)]
pub(crate) struct SourceScanResult {
    pub(crate) assets: Vec<CatalogAsset>,
}

pub(crate) struct SourceScanWorkflow;

impl SourceScanWorkflow {
    pub(crate) async fn run(
        service: &AppService,
        params: SourceScanParams,
        cx: &TaskContext,
        skill_sources_only: bool,
    ) -> AppResult<SourceScanResult> {
        if cx.is_cancelled() {
            return Err(AppError::Cancelled("source scan cancelled".to_string()));
        }
        if params.dry_run {
            return Ok(SourceScanResult {
                assets: capabilities::catalog_assets_sqlx(
                    service.db.pool(),
                    service.tenant_id(),
                    params.kind,
                )
                .await?,
            });
        }

        let sources = if skill_sources_only {
            crate::backend::store::load_skill_sources_sqlx(service.db.pool(), service.tenant_id())
                .await?
        } else {
            crate::backend::store::load_sources_sqlx(service.db.pool(), service.tenant_id()).await?
        };
        let total = sources.len();
        let scan = if skill_sources_only {
            capabilities::scan_skill_source
        } else {
            capabilities::scan_source
        };
        capabilities::scan_selected_sources_with_progress(
            service.db.pool(),
            service.tenant_id(),
            sources,
            scan,
            |index, total, source| {
                if cx.is_cancelled() {
                    return Err(AppError::Cancelled("source scan cancelled".to_string()));
                }
                cx.progress().progress(
                    index as u64,
                    Some(total as u64),
                    Some(source.name.as_str()),
                );
                Ok(())
            },
        )
        .await?;
        if cx.is_cancelled() {
            return Err(AppError::Cancelled("source scan cancelled".to_string()));
        }
        cx.progress()
            .progress(total as u64, Some(total as u64), Some("completed"));
        Ok(SourceScanResult {
            assets: capabilities::catalog_assets_sqlx(
                service.db.pool(),
                service.tenant_id(),
                if skill_sources_only {
                    Some(AssetKind::Skill)
                } else {
                    params.kind
                },
            )
            .await?,
        })
    }
}

impl AppService {
    pub(crate) async fn refresh_recorded_assets(&self) -> AppResult<Vec<Asset>> {
        capabilities::refresh_recorded_assets(self.db.pool(), self.tenant_id()).await
    }

    pub(crate) async fn list_sources(&self) -> AppResult<Vec<Source>> {
        crate::backend::store::load_sources_sqlx(self.db.pool(), self.tenant_id()).await
    }

    pub(crate) async fn list_skill_sources(&self) -> AppResult<Vec<Source>> {
        crate::backend::store::load_skill_sources_sqlx(self.db.pool(), self.tenant_id()).await
    }

    pub(crate) async fn list_source_assets(
        &self,
        kind: Option<AssetKind>,
    ) -> AppResult<Vec<CatalogAsset>> {
        capabilities::source_assets_sqlx(self.db.pool(), self.tenant_id(), kind).await
    }

    pub(crate) async fn add_source(&self, source: SourceInput) -> AppResult<Source> {
        let catalog = self.runtime.target_catalog();
        let source = source_from_input(source, catalog.as_ref());
        crate::backend::store::upsert_source_sqlx_with_catalog(
            self.db.pool(),
            self.tenant_id(),
            &source,
            catalog.as_ref(),
        )
        .await?;
        Ok(source)
    }

    pub(crate) async fn update_source(&self, source: Source) -> AppResult<Source> {
        if is_protected_source(&source) {
            return Err(AppError::Conflict(
                "AssetIWeave-managed Skill sources cannot be edited".to_string(),
            ));
        }
        let catalog = self.runtime.target_catalog();
        let source =
            crate::backend::store::normalize_source_with_catalog(&source, catalog.as_ref());
        if !self
            .list_sources()
            .await?
            .iter()
            .any(|candidate| candidate.id == source.id)
        {
            return Err(AppError::NotFound(format!(
                "source not found: {}",
                source.id
            )));
        }
        crate::backend::store::upsert_source_sqlx_with_catalog(
            self.db.pool(),
            self.tenant_id(),
            &source,
            catalog.as_ref(),
        )
        .await?;
        Ok(source)
    }

    pub(crate) async fn delete_source(&self, id: String) -> AppResult<()> {
        self.remove_source(SourceRemoveParams {
            id,
            dry_run: false,
            yes: true,
        })
        .await
        .map(|_| ())
    }

    pub(crate) async fn add_source_with_options(
        &self,
        params: SourceAddParams,
    ) -> AppResult<Value> {
        let catalog = self.runtime.target_catalog();
        let source = source_from_input(params.source, catalog.as_ref());
        if params.dry_run {
            return Ok(json!({ "dry_run": true, "source": source }));
        }
        crate::backend::store::upsert_source_sqlx_with_catalog(
            self.db.pool(),
            self.tenant_id(),
            &source,
            catalog.as_ref(),
        )
        .await?;
        Ok(json!({ "dry_run": false, "source": source }))
    }

    pub(crate) async fn remove_source(&self, params: SourceRemoveParams) -> AppResult<Value> {
        if !params.dry_run && !params.yes {
            return Err(AppError::Validation(
                "source.remove requires --yes".to_string(),
            ));
        }
        let sources = self.list_sources().await?;
        let source = sources
            .into_iter()
            .find(|source| source.id == params.id)
            .ok_or_else(|| AppError::NotFound(format!("source not found: {}", params.id)))?;
        if is_protected_source(&source) {
            return Err(AppError::Conflict(
                "default Skill source is managed by AssetIWeave and cannot be deleted".to_string(),
            ));
        }
        if params.dry_run {
            return Ok(json!({ "removed": false, "dry_run": true, "source": source }));
        }
        crate::backend::store::delete_source_sqlx(self.db.pool(), self.tenant_id(), &source.id)
            .await?;
        capabilities::cleanup_orphan_asset_records(self.db.pool(), self.tenant_id()).await?;
        Ok(json!({ "removed": true, "source_id": source.id }))
    }

    pub(crate) async fn scan_sources(
        &self,
        params: SourceScanParams,
    ) -> AppResult<Vec<CatalogAsset>> {
        Ok(
            SourceScanWorkflow::run(self, params, &TaskContext::untracked(), false)
                .await?
                .assets,
        )
    }

    pub(crate) async fn scan_skill_sources(&self) -> AppResult<Vec<CatalogAsset>> {
        Ok(SourceScanWorkflow::run(
            self,
            SourceScanParams {
                kind: Some(AssetKind::Skill),
                dry_run: false,
            },
            &TaskContext::untracked(),
            true,
        )
        .await?
        .assets)
    }
}

fn is_protected_source(source: &Source) -> bool {
    source.id == "assetiweave-library-skills"
        || source.id == crate::backend::builtin_skills::SYSTEM_SKILL_SOURCE_ID
        || matches!(
            source.source_origin,
            SourceOrigin::AssetiweaveLibrary | SourceOrigin::AssetiweaveSystem
        )
}

fn source_from_input(
    source: SourceInput,
    catalog: &crate::backend::target_catalog::TargetCatalog,
) -> Source {
    let source = Source {
        id: source.id.unwrap_or_else(|| Uuid::new_v4().to_string()),
        name: source.name,
        kind: source.kind,
        root_path: source.root_path,
        scanner_kind: source.scanner_kind.unwrap_or(SourceScannerKind::Mixed),
        source_origin: source.source_origin.unwrap_or(SourceOrigin::LocalFolder),
        repo_root: source.repo_root,
        scan_root: source.scan_root.unwrap_or_default(),
        origin_app_kind: source.origin_app_kind,
        origin_provider_id: source.origin_provider_id,
        include_globs: source.include_globs,
        exclude_globs: source.exclude_globs,
        default_kind: source.default_kind,
        enabled: source.enabled,
        priority: source.priority,
        last_scanned_at: None,
        last_scan_status: Some("pending".to_string()),
    };
    crate::backend::store::normalize_source_with_catalog(&source, catalog)
}
