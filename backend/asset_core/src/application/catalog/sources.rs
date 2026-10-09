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

        // 阶段 1: 发现资产来源
        let sources = {
            let mut guard = cx.enter_stage(crate::backend::infrastructure::tasks::stages::DISCOVER);
            let raw_sources = if skill_sources_only {
                crate::backend::store::load_skill_sources_sqlx(
                    service.db.pool(),
                    service.tenant_id(),
                )
                .await?
            } else {
                crate::backend::store::load_sources_sqlx(service.db.pool(), service.tenant_id())
                    .await?
            };
            guard.record_metric("total_sources", raw_sources.len() as u64);
            raw_sources
        };
        let total = sources.len();
        let scan = if skill_sources_only {
            super::source_scanner::scan_skill_source
        } else {
            super::source_scanner::scan_source
        };

        // 阶段 2: 扫描资产目录
        {
            let mut guard =
                cx.enter_stage(crate::backend::infrastructure::tasks::stages::SCAN_FILES);
            let mut scanned_count = 0u64;
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
                    scanned_count += 1;
                    Ok(())
                },
            )
            .await?;
            guard.record_metric("scanned_sources", scanned_count);
        }

        if cx.is_cancelled() {
            return Err(AppError::Cancelled("source scan cancelled".to_string()));
        }

        // 阶段 3: 建立资产索引
        let assets = {
            let mut guard =
                cx.enter_stage(crate::backend::infrastructure::tasks::stages::PERSIST_INDEX);
            let result = super::catalog_ops::catalog_assets_sqlx(
                service.db.pool(),
                service.tenant_id(),
                if skill_sources_only {
                    Some(AssetKind::Skill)
                } else {
                    params.kind
                },
            )
            .await?;
            guard.record_metric("catalog_assets", result.len() as u64);
            result
        };

        cx.progress()
            .progress(total as u64, Some(total as u64), Some("completed"));
        Ok(assets)
    }
}

impl AppService {
    pub async fn refresh_recorded_assets(&self) -> AppResult<Vec<Asset>> {
        super::source_scanner::refresh_recorded_assets(self.db.pool(), self.tenant_id()).await
    }

    pub async fn list_sources(&self) -> AppResult<Vec<Source>> {
        Ok(crate::backend::store::load_sources_sqlx(self.db.pool(), self.tenant_id()).await?)
    }

    pub async fn list_skill_sources(&self) -> AppResult<Vec<Source>> {
        Ok(
            crate::backend::store::load_skill_sources_sqlx(self.db.pool(), self.tenant_id())
                .await?,
        )
    }

    pub async fn list_source_assets(
        &self,
        kind: Option<AssetKind>,
    ) -> AppResult<Vec<CatalogAsset>> {
        super::catalog_ops::source_assets_sqlx(self.db.pool(), self.tenant_id(), kind).await
    }

    pub async fn add_source(&self, source: SourceInput) -> AppResult<Source> {
        let catalog = self.runtime.target_catalog();
        let source = source_from_input(source, catalog.as_ref());
        crate::backend::store::upsert_source_sqlx(self.db.pool(), self.tenant_id(), &source)
            .await?;
        Ok(source)
    }

    pub async fn update_source(&self, source: Source) -> AppResult<Source> {
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

    pub async fn delete_source(&self, id: String) -> AppResult<()> {
        self.remove_source(SourceRemoveParams {
            id,
            dry_run: false,
            yes: true,
        })
        .await
        .map(|_| ())
    }

    pub async fn add_source_with_options(&self, params: SourceAddParams) -> AppResult<Value> {
        let catalog = self.runtime.target_catalog();
        let source = source_from_input(params.source, catalog.as_ref());
        if params.dry_run {
            return Ok(json!({ "dry_run": true, "source": source }));
        }
        crate::backend::store::upsert_source_sqlx(self.db.pool(), self.tenant_id(), &source)
            .await?;
        Ok(json!({ "dry_run": false, "source": source }))
    }

    pub async fn remove_source(&self, params: SourceRemoveParams) -> AppResult<Value> {
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

    pub async fn scan_sources(&self, params: SourceScanParams) -> AppResult<Vec<CatalogAsset>> {
        self.scan_sources_internal(params, false).await
    }

    pub async fn scan_skill_sources(&self) -> AppResult<Vec<CatalogAsset>> {
        self.scan_sources_internal(
            SourceScanParams {
                kind: Some(AssetKind::Skill),
                dry_run: false,
            },
            true,
        )
        .await
    }

    async fn scan_sources_internal(
        &self,
        params: SourceScanParams,
        skill_sources_only: bool,
    ) -> AppResult<Vec<CatalogAsset>> {
        let pipeline = crate::backend::infrastructure::tasks::PipelineDescriptor::builder()
            .stage(
                crate::backend::infrastructure::tasks::stages::DISCOVER,
                "发现资产来源",
            )
            .stage(
                crate::backend::infrastructure::tasks::stages::SCAN_FILES,
                "扫描资产目录",
            )
            .stage(
                crate::backend::infrastructure::tasks::stages::PERSIST_INDEX,
                "建立资产索引",
            )
            .build();
        let spec = crate::backend::infrastructure::tasks::TaskSpec::new(
            crate::backend::infrastructure::tasks::TaskKind::Scan,
            Some(format!("scan_sources:{}", self.tenant_id())),
        )
        .with_pipeline(pipeline)
        .with_title(if skill_sources_only {
            "扫描 Skill 资产来源"
        } else {
            "扫描资产来源"
        });

        let service = self.clone();
        let snapshot = self
            .runtime
            .task_runtime()
            .run_inline(spec, move |cx| async move {
                let assets = SourceScanWorkflow::run(&service, params, &cx, skill_sources_only)
                    .await
                    .map_err(crate::backend::infrastructure::error::InfraError::external)?;
                let summary = format!("扫描完成，共 {} 项资产", assets.len());
                Ok(
                    crate::backend::infrastructure::tasks::TaskOutput::with_summary(
                        assets, summary,
                    ),
                )
            })
            .await?;

        if let Some(err) = snapshot.error {
            return Err(AppError::External(err.message));
        }
        if let Some(val) = snapshot.result {
            if let Ok(assets) = serde_json::from_value(val) {
                return Ok(assets);
            }
        }
        Ok(Vec::new())
    }

    pub fn start_source_scan_pipeline(
        &self,
        params: SourceScanParams,
        skill_sources_only: bool,
    ) -> AppResult<crate::backend::infrastructure::tasks::TaskHandle<Vec<CatalogAsset>>> {
        let pipeline = crate::backend::infrastructure::tasks::PipelineDescriptor::builder()
            .stage(
                crate::backend::infrastructure::tasks::stages::DISCOVER,
                "发现资产来源",
            )
            .stage(
                crate::backend::infrastructure::tasks::stages::SCAN_FILES,
                "扫描资产目录",
            )
            .stage(
                crate::backend::infrastructure::tasks::stages::PERSIST_INDEX,
                "建立资产索引",
            )
            .build();
        let spec = crate::backend::infrastructure::tasks::TaskSpec::new(
            crate::backend::infrastructure::tasks::TaskKind::Scan,
            Some(format!("scan_sources:{}", self.tenant_id())),
        )
        .with_pipeline(pipeline)
        .with_title(if skill_sources_only {
            "扫描 Skill 资产来源"
        } else {
            "扫描资产来源"
        });

        let service = self.clone();
        self.spawn_pipeline_task(spec, move |cx| async move {
            let assets = SourceScanWorkflow::run(&service, params, &cx, skill_sources_only)
                .await
                .map_err(crate::backend::infrastructure::error::InfraError::external)?;
            let summary = format!("扫描完成，共 {} 项资产", assets.len());
            Ok(crate::backend::infrastructure::tasks::TaskOutput::with_summary(assets, summary))
        })
    }

    pub async fn scan_sources_with_task_context(
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

pub fn normalize_source(source: &Source) -> Source {
    normalize_source_inner(source, None)
}

pub fn normalize_source_with_catalog(
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
