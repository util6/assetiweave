use super::prelude::*;

pub(crate) const SKILL_BACKUP_SOURCE_ID: &str = "assetiweave-library-skills";

pub(crate) fn assetiweave_library_source_for_tenant(tenant_id: &str) -> Source {
    let root_path = default_skill_backup_root_for_tenant(tenant_id)
        .map(|path| path.to_string_lossy().to_string())
        .unwrap_or_else(|_| format!("~/.assetiweave/tenants/{tenant_id}/library/skills"));
    assetiweave_library_source_with_root(root_path)
}

pub(crate) fn assetiweave_library_source_with_root(root_path: String) -> Source {
    Source {
        id: SKILL_BACKUP_SOURCE_ID.to_string(),
        name: "AssetIWeave Skill Backup Library".to_string(),
        kind: SourceKind::Local,
        root_path,
        scanner_kind: SourceScannerKind::Skill,
        source_origin: SourceOrigin::AssetiweaveLibrary,
        repo_root: None,
        scan_root: String::new(),
        origin_app_kind: None,
        include_globs: vec!["**/SKILL.md".to_string()],
        exclude_globs: vec![
            "**/.git/**".to_string(),
            "**/node_modules/**".to_string(),
            "**/target/**".to_string(),
            "**/dist/**".to_string(),
        ],
        default_kind: Some(AssetKind::Skill),
        enabled: true,
        priority: -100,
        last_scanned_at: None,
        last_scan_status: Some("pending".to_string()),
    }
}

pub(crate) fn skill_backup_root_sqlx(
    db: &crate::backend::store::Database,
    tenant_id: &str,
) -> AppResult<PathBuf> {
    let pool = db.pool().clone();
    let tenant_id = tenant_id.to_string();
    let tenant_id_for_query = tenant_id.clone();
    let sources = db.block_on(async move {
        crate::backend::store::load_sources_sqlx(&pool, &tenant_id_for_query).await
    })?;
    let root_path = skill_backup_root_path(&tenant_id, sources);
    let root = expand_path(&root_path)?;
    fs::create_dir_all(&root).map_err(|error| error.to_string())?;
    Ok(root)
}

pub(crate) fn skill_backup_settings_sqlx(
    db: &crate::backend::store::Database,
    tenant_id: &str,
) -> AppResult<SkillBackupSettings> {
    let pool = db.pool().clone();
    let tenant_id = tenant_id.to_string();
    let tenant_id_for_query = tenant_id.clone();
    let sources = db.block_on(async move {
        crate::backend::store::load_sources_sqlx(&pool, &tenant_id_for_query).await
    })?;
    build_skill_backup_settings(&tenant_id, sources)
}

fn skill_backup_root_path(tenant_id: &str, sources: Vec<Source>) -> String {
    sources
        .into_iter()
        .find(|source| source.id == SKILL_BACKUP_SOURCE_ID)
        .map(|source| source.root_path)
        .unwrap_or_else(|| {
            default_skill_backup_root_for_tenant(tenant_id)
                .map(|path| path.to_string_lossy().to_string())
                .unwrap_or_else(|_| format!("~/.assetiweave/tenants/{tenant_id}/library/skills"))
        })
}

fn build_skill_backup_settings(
    tenant_id: &str,
    sources: Vec<Source>,
) -> AppResult<SkillBackupSettings> {
    let default_root = default_skill_backup_root_for_tenant(tenant_id)?;
    let source = sources
        .into_iter()
        .find(|source| source.id == SKILL_BACKUP_SOURCE_ID)
        .unwrap_or_else(|| assetiweave_library_source_for_tenant(tenant_id));
    let expanded_root = expand_path(&source.root_path)?;
    Ok(SkillBackupSettings {
        root_path: source.root_path,
        expanded_root_path: expanded_root.to_string_lossy().to_string(),
        default_root_path: default_root.to_string_lossy().to_string(),
        is_default_root: same_path_or_text(&expanded_root, &default_root),
        exists: expanded_root.exists(),
    })
}

pub(crate) fn catalog_assets_sqlx(
    db: &crate::backend::store::Database,
    tenant_id: &str,
    kind: Option<AssetKind>,
) -> AppResult<Vec<CatalogAsset>> {
    let pool = db.pool().clone();
    let tenant_id = tenant_id.to_string();
    db.block_on(async move {
        let assets = crate::backend::store::load_assets_sqlx(&pool, &tenant_id, kind).await?;
        let sources = crate::backend::store::load_sources_sqlx(&pool, &tenant_id).await?;
        let assets = filter_unavailable_backup_library_assets(assets, &sources);
        AppResult::Ok(build_catalog_assets(assets, &sources))
    })
}

pub(crate) fn catalog_visible_assets_sqlx(
    db: &crate::backend::store::Database,
    tenant_id: &str,
    kind: Option<AssetKind>,
) -> AppResult<Vec<Asset>> {
    let pool = db.pool().clone();
    let tenant_id = tenant_id.to_string();
    db.block_on(async move {
        let assets = crate::backend::store::load_assets_sqlx(&pool, &tenant_id, kind).await?;
        let sources = crate::backend::store::load_sources_sqlx(&pool, &tenant_id).await?;
        let assets = filter_unavailable_backup_library_assets(assets, &sources);
        AppResult::Ok(
            build_catalog_asset_entries(assets, &sources)
                .into_iter()
                .map(|catalog_asset| catalog_asset.asset)
                .collect(),
        )
    })
}

fn filter_unavailable_backup_library_assets(assets: Vec<Asset>, sources: &[Source]) -> Vec<Asset> {
    let source_by_id = sources
        .iter()
        .map(|source| (source.id.as_str(), source))
        .collect::<HashMap<_, _>>();
    assets
        .into_iter()
        .filter(|asset| {
            let source = source_by_id.get(asset.source_id.as_str()).copied();
            !unavailable_backup_library_asset(asset, source)
        })
        .collect()
}

pub(crate) fn build_catalog_assets(assets: Vec<Asset>, sources: &[Source]) -> Vec<CatalogAsset> {
    let mut catalog_assets = build_catalog_asset_entries(assets, sources);
    attach_git_repository_info(&mut catalog_assets);
    catalog_assets
}

fn build_catalog_asset_entries(assets: Vec<Asset>, sources: &[Source]) -> Vec<CatalogAsset> {
    let source_by_id = sources
        .iter()
        .map(|source| (source.id.as_str(), source))
        .collect::<HashMap<_, _>>();
    let mut content_groups: BTreeMap<String, Vec<Asset>> = BTreeMap::new();
    let mut without_identity = Vec::new();

    for asset in assets {
        let source = source_by_id.get(asset.source_id.as_str()).copied();
        if asset.kind == AssetKind::Skill {
            if let Some(content_hash) = asset.content_hash.clone().filter(|hash| !hash.is_empty()) {
                content_groups.entry(content_hash).or_default().push(asset);
                continue;
            }
        }
        without_identity.push(CatalogAsset {
            backup_status: standalone_backup_status(&asset, source),
            repository: None,
            asset,
        });
    }

    let mut catalog_assets = without_identity;
    for mut group in content_groups.into_values() {
        if group.len() == 1 {
            let asset = group.remove(0);
            catalog_assets.push(CatalogAsset {
                backup_status: standalone_backup_status(
                    &asset,
                    source_by_id.get(asset.source_id.as_str()).copied(),
                ),
                repository: None,
                asset,
            });
            continue;
        }

        group.sort_by(|left, right| {
            let left_score =
                canonical_asset_score(left, source_by_id.get(left.source_id.as_str()).copied());
            let right_score =
                canonical_asset_score(right, source_by_id.get(right.source_id.as_str()).copied());
            left_score
                .cmp(&right_score)
                .then_with(|| left.name.cmp(&right.name))
                .then_with(|| left.absolute_path.cmp(&right.absolute_path))
        });

        let canonical = group.remove(0);
        let hidden_asset_ids = group
            .iter()
            .map(|asset| asset.id.clone())
            .collect::<Vec<_>>();
        let backup_path = std::iter::once(&canonical)
            .chain(group.iter())
            .find(|asset| {
                backup_entry_state(asset, source_by_id.get(asset.source_id.as_str()).copied())
                    == Some(SkillBackupState::BackedUp)
            })
            .map(|asset| asset.absolute_path.clone());
        let backup_status = if let Some(backup_path) = backup_path {
            Some(SkillBackupAssetStatus {
                state: SkillBackupState::BackedUp,
                backup_path: Some(backup_path),
                hidden_asset_ids,
            })
        } else {
            standalone_backup_status(
                &canonical,
                source_by_id.get(canonical.source_id.as_str()).copied(),
            )
            .map(|mut status| {
                status.hidden_asset_ids = hidden_asset_ids;
                status
            })
        };

        catalog_assets.push(CatalogAsset {
            asset: canonical,
            backup_status,
            repository: None,
        });
    }

    catalog_assets.sort_by(|left, right| {
        left.asset
            .name
            .cmp(&right.asset.name)
            .then_with(|| left.asset.relative_path.cmp(&right.asset.relative_path))
    });
    catalog_assets
}

fn attach_git_repository_info(catalog_assets: &mut [CatalogAsset]) {
    let mut repository_by_root = HashMap::new();
    for catalog_asset in catalog_assets {
        let asset_path = Path::new(&catalog_asset.asset.absolute_path);
        let Some(repository_root) = find_git_root(asset_path) else {
            continue;
        };
        let repository = repository_by_root
            .entry(repository_root.clone())
            .or_insert_with(|| git_repository_for_path(&repository_root));
        catalog_asset.repository = repository.clone().map(|mut repository| {
            repository.web_url = repository
                .remote_url
                .as_deref()
                .and_then(|remote| git_browser_url(remote, &repository_root, asset_path));
            repository
        });
    }
}

fn standalone_backup_status(
    asset: &Asset,
    source: Option<&Source>,
) -> Option<SkillBackupAssetStatus> {
    backup_entry_state(asset, source).map(|state| SkillBackupAssetStatus {
        state,
        backup_path: Some(asset.absolute_path.clone()),
        hidden_asset_ids: Vec::new(),
    })
}

fn unavailable_backup_library_asset(asset: &Asset, source: Option<&Source>) -> bool {
    if asset.kind != AssetKind::Skill || backup_entry_state(asset, source).is_none() {
        return false;
    }

    let asset_path = Path::new(&asset.absolute_path);
    if !asset_path.exists() {
        return true;
    }

    source
        .and_then(|source| expand_path(&source.root_path).ok())
        .is_some_and(|root| !path_is_inside_or_same(&root, asset_path))
}

fn path_is_inside_or_same(root: &Path, path: &Path) -> bool {
    let normalized_root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let normalized_path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    normalized_path == normalized_root || normalized_path.starts_with(&normalized_root)
}

fn backup_entry_state(asset: &Asset, source: Option<&Source>) -> Option<SkillBackupState> {
    let source = source?;
    if source.id != SKILL_BACKUP_SOURCE_ID
        && !matches!(source.source_origin, SourceOrigin::AssetiweaveLibrary)
    {
        return None;
    }

    if asset.relative_path.starts_with("downloaded/")
        || asset.relative_path.starts_with("imported/")
    {
        return Some(SkillBackupState::Downloaded);
    }
    if asset.relative_path.starts_with("backed-up/") {
        return Some(SkillBackupState::BackedUp);
    }
    None
}

fn canonical_asset_score(asset: &Asset, source: Option<&Source>) -> u8 {
    let Some(source) = source else {
        return 50;
    };
    match source.source_origin {
        SourceOrigin::AppTarget | SourceOrigin::AppLocal => 40,
        SourceOrigin::AssetiweaveLibrary => match backup_entry_state(asset, Some(source)) {
            Some(SkillBackupState::Downloaded) => 20,
            Some(SkillBackupState::BackedUp) => 30,
            None => 25,
        },
        SourceOrigin::GitRepo | SourceOrigin::LocalFolder | SourceOrigin::Custom => 0,
    }
}
