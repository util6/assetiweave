use crate::backend::application::prelude::*;
use crate::backend::application::{AppError, AppResult};
use crate::backend::infrastructure::tasks::TaskContext;

struct SourceScanWorkflow;

impl SourceScanWorkflow {
    async fn run(
        service: &AppService,
        params: SourceScanParams,
        cx: &TaskContext,
        skill_sources_only: bool,
    ) -> AppResult<Vec<CatalogAsset>> {
        if cx.is_cancelled() {
            return Err(AppError::Cancelled("source scan cancelled".to_string()));
        }
        if params.dry_run {
            return super::catalog_ops::catalog_assets_sqlx(
                service.db.pool(),
                service.tenant_id(),
                params.kind,
            )
            .await;
        }

        let sources = if skill_sources_only {
            crate::backend::store::load_skill_sources_sqlx(service.db.pool(), service.tenant_id())
                .await?
        } else {
            crate::backend::store::load_sources_sqlx(service.db.pool(), service.tenant_id()).await?
        };
        let total = sources.len();
        let scan = if skill_sources_only {
            super::source_scanner::scan_skill_source
        } else {
            super::source_scanner::scan_source
        };
        super::source_scanner::scan_selected_sources_with_progress(
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
        super::catalog_ops::catalog_assets_sqlx(
            service.db.pool(),
            service.tenant_id(),
            if skill_sources_only {
                Some(AssetKind::Skill)
            } else {
                params.kind
            },
        )
        .await
    }
}

impl AppService {
    pub(crate) async fn refresh_recorded_assets(&self) -> AppResult<Vec<Asset>> {
        super::source_scanner::refresh_recorded_assets(self.db.pool(), self.tenant_id()).await
    }

    pub(crate) async fn list_sources(&self) -> AppResult<Vec<Source>> {
        Ok(crate::backend::store::load_sources_sqlx(self.db.pool(), self.tenant_id()).await?)
    }

    pub(crate) async fn list_skill_sources(&self) -> AppResult<Vec<Source>> {
        Ok(
            crate::backend::store::load_skill_sources_sqlx(self.db.pool(), self.tenant_id())
                .await?,
        )
    }

    pub(crate) async fn list_source_assets(
        &self,
        kind: Option<AssetKind>,
    ) -> AppResult<Vec<CatalogAsset>> {
        super::catalog_ops::source_assets_sqlx(self.db.pool(), self.tenant_id(), kind).await
    }

    pub(crate) async fn add_source(&self, source: SourceInput) -> AppResult<Source> {
        let catalog = self.runtime.target_catalog();
        let source = source_from_input(source, catalog.as_ref());
        crate::backend::store::upsert_source_sqlx(self.db.pool(), self.tenant_id(), &source)
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
        let source = normalize_source_with_catalog(&source, catalog.as_ref());
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
        crate::backend::store::upsert_source_sqlx(self.db.pool(), self.tenant_id(), &source)
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
        crate::backend::store::upsert_source_sqlx(self.db.pool(), self.tenant_id(), &source)
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
        super::source_scanner::cleanup_orphan_asset_records(self.db.pool(), self.tenant_id())
            .await?;
        Ok(json!({ "removed": true, "source_id": source.id }))
    }

    pub(crate) async fn scan_sources(
        &self,
        params: SourceScanParams,
    ) -> AppResult<Vec<CatalogAsset>> {
        self.scan_sources_with_task_context(params, &TaskContext::untracked(), false)
            .await
    }

    pub(crate) async fn scan_skill_sources(&self) -> AppResult<Vec<CatalogAsset>> {
        self.scan_sources_with_task_context(
            SourceScanParams {
                kind: Some(AssetKind::Skill),
                dry_run: false,
            },
            &TaskContext::untracked(),
            true,
        )
        .await
    }

    pub(crate) async fn scan_sources_with_task_context(
        &self,
        params: SourceScanParams,
        context: &TaskContext,
        skill_sources_only: bool,
    ) -> AppResult<Vec<CatalogAsset>> {
        SourceScanWorkflow::run(self, params, context, skill_sources_only).await
    }
}

fn is_protected_source(source: &Source) -> bool {
    source.id == "assetiweave-library-skills"
        || source.id == super::builtin_skills::SYSTEM_SKILL_SOURCE_ID
        || matches!(
            source.source_origin,
            SourceOrigin::AssetiweaveLibrary | SourceOrigin::AssetiweaveSystem
        )
}

fn source_from_input(
    source: SourceInput,
    catalog: &crate::backend::application::mounting::target_catalog::TargetCatalog,
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
    normalize_source_with_catalog(&source, catalog)
}

pub(crate) fn normalize_source(source: &Source) -> Source {
    normalize_source_inner(source, None)
}

pub(crate) fn normalize_source_with_catalog(
    source: &Source,
    catalog: &crate::backend::application::mounting::target_catalog::TargetCatalog,
) -> Source {
    normalize_source_inner(source, Some(catalog))
}

fn normalize_source_inner(
    source: &Source,
    catalog: Option<&crate::backend::application::mounting::target_catalog::TargetCatalog>,
) -> Source {
    let mut source = source.clone();
    normalize_source_paths(&mut source);

    if matches!(source.scanner_kind, SourceScannerKind::Mixed) && is_skill_like_source(&source) {
        source.scanner_kind = SourceScannerKind::Skill;
    }

    if source.id == "assetiweave-library-skills" {
        source.source_origin = SourceOrigin::AssetiweaveLibrary;
        source.scanner_kind = SourceScannerKind::Skill;
        source.repo_root = None;
        source.scan_root = String::new();
        source.origin_app_kind = None;
        source.origin_provider_id = None;
        return source;
    }

    if source.id == super::builtin_skills::SYSTEM_SKILL_SOURCE_ID {
        return super::builtin_skills::system_skill_source().unwrap_or(source);
    }

    let Ok(root_path) = crate::backend::infrastructure::path_utils::expand_path(&source.root_path)
    else {
        return source;
    };

    if crate::backend::infrastructure::path_utils::is_app_library_path(&root_path) {
        source.source_origin = SourceOrigin::AssetiweaveLibrary;
        source.scanner_kind = SourceScannerKind::Skill;
        source.repo_root = None;
        source.scan_root = String::new();
        source.origin_app_kind = None;
        source.origin_provider_id = None;
        return source;
    }

    if let Some(catalog) = catalog {
        if let Some((provider_id, app_kind)) =
            crate::backend::infrastructure::path_utils::detect_target_provider(&root_path, catalog)
        {
            source.source_origin = SourceOrigin::AppTarget;
            source.scanner_kind = SourceScannerKind::Skill;
            source.repo_root = None;
            source.scan_root = String::new();
            source.origin_app_kind = app_kind;
            source.origin_provider_id = Some(provider_id);
            return source;
        }
    }

    if let Some(git_root) = crate::backend::infrastructure::path_utils::find_git_root(&root_path) {
        source.source_origin = SourceOrigin::GitRepo;
        source.repo_root =
            crate::backend::infrastructure::path_utils::normalize_std_path_for_storage(&git_root)
                .ok();
        source.scan_root = root_path
            .strip_prefix(&git_root)
            .ok()
            .map(crate::backend::infrastructure::path_utils::normalize_relative_path)
            .unwrap_or_default();
    }
    source
}

fn normalize_source_paths(source: &mut Source) {
    if let Ok(root_path) =
        crate::backend::infrastructure::path_utils::normalize_path_for_storage(&source.root_path)
    {
        source.root_path = root_path;
    }
    source.repo_root = source.repo_root.as_deref().map(|path| {
        crate::backend::infrastructure::path_utils::normalize_path_for_storage(path)
            .unwrap_or_else(|_| path.to_string())
    });
}

fn is_skill_like_source(source: &Source) -> bool {
    source.default_kind == Some(AssetKind::Skill)
        || source
            .include_globs
            .iter()
            .any(|glob| glob.to_ascii_lowercase().contains("skill.md"))
}

#[cfg(test)]
#[path = "sources_tests.rs"]
mod tests;
