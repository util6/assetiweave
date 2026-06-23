use super::prelude::*;

impl AppService {
    pub(crate) fn refresh_recorded_assets(&self) -> AppResult<Vec<Asset>> {
        capabilities::refresh_recorded_assets(&self.db)
    }

    pub(crate) fn list_sources(&self) -> AppResult<Vec<Source>> {
        let pool = self.db.pool().clone();
        self.db
            .block_on(async move { crate::backend::store::load_sources_sqlx(&pool).await })
    }

    pub(crate) fn list_skill_sources(&self) -> AppResult<Vec<Source>> {
        let pool = self.db.pool().clone();
        self.db
            .block_on(async move { crate::backend::store::load_skill_sources_sqlx(&pool).await })
    }

    pub(crate) fn add_source(&self, source: SourceInput) -> AppResult<Source> {
        let source = source_from_input(source);
        let pool = self.db.pool().clone();
        let source_to_save = source.clone();
        self.db.block_on(async move {
            crate::backend::store::upsert_source_sqlx(&pool, &source_to_save).await
        })?;
        Ok(source)
    }

    pub(crate) fn update_source(&self, source: Source) -> AppResult<Source> {
        let source = crate::backend::store::normalize_source(&source);
        if !self
            .list_sources()?
            .iter()
            .any(|candidate| candidate.id == source.id)
        {
            return Err(format!("source not found: {}", source.id));
        }
        let pool = self.db.pool().clone();
        let source_to_save = source.clone();
        self.db.block_on(async move {
            crate::backend::store::upsert_source_sqlx(&pool, &source_to_save).await
        })?;
        Ok(source)
    }

    pub(crate) fn delete_source(&self, id: String) -> AppResult<()> {
        self.remove_source(SourceRemoveParams {
            id,
            dry_run: false,
            yes: true,
        })
        .map(|_| ())
    }

    pub(crate) fn add_source_with_options(&self, params: SourceAddParams) -> AppResult<Value> {
        let source = source_from_input(params.source);
        if params.dry_run {
            return Ok(json!({ "dry_run": true, "source": source }));
        }
        let pool = self.db.pool().clone();
        let source_to_save = source.clone();
        self.db.block_on(async move {
            crate::backend::store::upsert_source_sqlx(&pool, &source_to_save).await
        })?;
        Ok(json!({ "dry_run": false, "source": source }))
    }

    pub(crate) fn remove_source(&self, params: SourceRemoveParams) -> AppResult<Value> {
        if !params.dry_run && !params.yes {
            return Err("source.remove requires --yes".to_string());
        }
        let sources = self.list_sources()?;
        let source = sources
            .into_iter()
            .find(|source| source.id == params.id)
            .ok_or_else(|| format!("source not found: {}", params.id))?;
        if is_protected_source(&source) {
            return Err(
                "default Skill source is managed by AssetIWeave and cannot be deleted".to_string(),
            );
        }
        if params.dry_run {
            return Ok(json!({ "removed": false, "dry_run": true, "source": source }));
        }
        let pool = self.db.pool().clone();
        let source_id = source.id.clone();
        self.db.block_on(async move {
            crate::backend::store::delete_source_sqlx(&pool, &source_id).await
        })?;
        capabilities::cleanup_orphan_asset_records(&self.db)?;
        Ok(json!({ "removed": true, "source_id": source.id }))
    }

    pub(crate) fn scan_sources(&self, params: SourceScanParams) -> AppResult<Vec<CatalogAsset>> {
        if params.dry_run {
            return capabilities::catalog_assets_sqlx(&self.db, params.kind);
        }
        capabilities::refresh_all_sources(&self.db)?;
        capabilities::catalog_assets_sqlx(&self.db, params.kind)
    }

    pub(crate) fn scan_skill_sources(&self) -> AppResult<Vec<CatalogAsset>> {
        let pool = self.db.pool().clone();
        let sources = self
            .db
            .block_on(async move { crate::backend::store::load_skill_sources_sqlx(&pool).await })?;
        capabilities::scan_selected_sources(
            &self.db,
            sources,
            crate::backend::scanner::scan_skill_source,
        )?;
        capabilities::catalog_assets_sqlx(&self.db, Some(AssetKind::Skill))
    }
}

fn is_protected_source(source: &Source) -> bool {
    source.id == "assetiweave-library-skills"
        || matches!(source.source_origin, SourceOrigin::AssetiweaveLibrary)
}

fn source_from_input(source: SourceInput) -> Source {
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
        include_globs: source.include_globs,
        exclude_globs: source.exclude_globs,
        default_kind: source.default_kind,
        enabled: source.enabled,
        priority: source.priority,
        last_scanned_at: None,
        last_scan_status: Some("pending".to_string()),
    };
    crate::backend::store::normalize_source(&source)
}
