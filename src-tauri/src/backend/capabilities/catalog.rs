use super::prelude::*;

pub(crate) const SKILL_BACKUP_SOURCE_ID: &str = "assetiweave-library-skills";

pub(crate) fn assetiweave_library_source() -> Source {
    let root_path = default_skill_backup_root()
        .map(|path| path.to_string_lossy().to_string())
        .unwrap_or_else(|_| "~/.assetiweave/library/skills".to_string());
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

pub(crate) fn skill_backup_root_sqlx(db: &crate::backend::store::Database) -> AppResult<PathBuf> {
    let pool = db.pool().clone();
    let sources =
        db.block_on(async move { crate::backend::store::load_sources_sqlx(&pool).await })?;
    let root_path = skill_backup_root_path(sources);
    let root = expand_path(&root_path)?;
    fs::create_dir_all(&root).map_err(|error| error.to_string())?;
    Ok(root)
}

pub(crate) fn skill_backup_settings_sqlx(
    db: &crate::backend::store::Database,
) -> AppResult<SkillBackupSettings> {
    let pool = db.pool().clone();
    let sources =
        db.block_on(async move { crate::backend::store::load_sources_sqlx(&pool).await })?;
    build_skill_backup_settings(sources)
}

fn skill_backup_root_path(sources: Vec<Source>) -> String {
    sources
        .into_iter()
        .find(|source| source.id == SKILL_BACKUP_SOURCE_ID)
        .map(|source| source.root_path)
        .unwrap_or_else(|| {
            default_skill_backup_root()
                .map(|path| path.to_string_lossy().to_string())
                .unwrap_or_else(|_| "~/.assetiweave/library/skills".to_string())
        })
}

fn build_skill_backup_settings(sources: Vec<Source>) -> AppResult<SkillBackupSettings> {
    let default_root = default_skill_backup_root()?;
    let source = sources
        .into_iter()
        .find(|source| source.id == SKILL_BACKUP_SOURCE_ID)
        .unwrap_or_else(|| assetiweave_library_source());
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
    kind: Option<AssetKind>,
) -> AppResult<Vec<CatalogAsset>> {
    let pool = db.pool().clone();
    db.block_on(async move {
        let assets = crate::backend::store::load_assets_sqlx(&pool, kind).await?;
        let sources = crate::backend::store::load_sources_sqlx(&pool).await?;
        AppResult::Ok(build_catalog_assets(assets, &sources))
    })
}

pub(crate) fn catalog_visible_assets_sqlx(
    db: &crate::backend::store::Database,
    kind: Option<AssetKind>,
) -> AppResult<Vec<Asset>> {
    let pool = db.pool().clone();
    db.block_on(async move {
        let assets = crate::backend::store::load_assets_sqlx(&pool, kind).await?;
        let sources = crate::backend::store::load_sources_sqlx(&pool).await?;
        AppResult::Ok(
            build_catalog_asset_entries(assets, &sources)
                .into_iter()
                .map(|catalog_asset| catalog_asset.asset)
                .collect(),
        )
    })
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
        if asset.kind == AssetKind::Skill {
            if let Some(content_hash) = asset.content_hash.clone().filter(|hash| !hash.is_empty()) {
                content_groups.entry(content_hash).or_default().push(asset);
                continue;
            }
        }
        without_identity.push(CatalogAsset {
            backup_status: standalone_backup_status(
                &asset,
                source_by_id.get(asset.source_id.as_str()).copied(),
            ),
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
