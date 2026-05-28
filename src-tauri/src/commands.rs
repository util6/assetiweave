use crate::{
    executor,
    path_utils::{app_library_skill_root, expand_path},
    planner, platform, scanner, store, targeting,
    types::{
        AppOverview, AppResult, AppShortcut, AppState, AssetMountStatus, ExecutionResult,
        NavigationModel, PhysicalMountStateDto, SourceInput,
    },
};
use assetiweave_core::{
    Asset, AssetKind, AssetMount, DeploymentPlan, DeploymentStrategy, Source, SourceKind,
    SourceOrigin, SourceScannerKind, TargetProfile,
};
use chrono::Utc;
use std::{collections::HashMap, fs, path::Path};
use tauri::State;
use uuid::Uuid;
use walkdir::WalkDir;

#[tauri::command]
pub(crate) fn get_app_overview(state: State<'_, AppState>) -> AppResult<AppOverview> {
    let _guard = state.lock.lock().map_err(|error| error.to_string())?;
    let conn = store::open_initialized(&state.db_path)?;
    Ok(AppOverview {
        source_count: store::count_rows(&conn, "sources")?,
        asset_count: store::count_rows(&conn, "assets")?,
        profile_count: store::count_rows(&conn, "profiles")?,
        last_scan_status: store::latest_scan_status(&conn)?,
    })
}

#[tauri::command]
pub(crate) fn list_assets(state: State<'_, AppState>) -> AppResult<Vec<Asset>> {
    let _guard = state.lock.lock().map_err(|error| error.to_string())?;
    let conn = store::open_initialized(&state.db_path)?;
    store::load_assets(&conn)
}

#[tauri::command]
pub(crate) fn list_sources(state: State<'_, AppState>) -> AppResult<Vec<Source>> {
    let _guard = state.lock.lock().map_err(|error| error.to_string())?;
    let conn = store::open_initialized(&state.db_path)?;
    store::load_sources(&conn)
}

#[tauri::command]
pub(crate) fn list_skill_sources(state: State<'_, AppState>) -> AppResult<Vec<Source>> {
    let _guard = state.lock.lock().map_err(|error| error.to_string())?;
    let conn = store::open_initialized(&state.db_path)?;
    store::load_skill_sources(&conn)
}

#[tauri::command]
pub(crate) fn create_source(state: State<'_, AppState>, source: SourceInput) -> AppResult<Source> {
    let _guard = state.lock.lock().map_err(|error| error.to_string())?;
    let conn = store::open_initialized(&state.db_path)?;
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
    let source = store::normalize_source(&source);
    store::upsert_source(&conn, &source)?;
    Ok(source)
}

#[tauri::command]
pub(crate) fn update_source(state: State<'_, AppState>, source: Source) -> AppResult<Source> {
    let _guard = state.lock.lock().map_err(|error| error.to_string())?;
    let conn = store::open_initialized(&state.db_path)?;
    let source = store::normalize_source(&source);
    store::upsert_source(&conn, &source)?;
    Ok(source)
}

#[tauri::command]
pub(crate) fn delete_source(state: State<'_, AppState>, id: String) -> AppResult<()> {
    let _guard = state.lock.lock().map_err(|error| error.to_string())?;
    let conn = store::open_initialized(&state.db_path)?;
    store::delete_source(&conn, &id)?;
    cleanup_orphan_asset_records(&conn)
}

#[tauri::command]
pub(crate) fn list_profiles(state: State<'_, AppState>) -> AppResult<Vec<TargetProfile>> {
    let _guard = state.lock.lock().map_err(|error| error.to_string())?;
    let conn = store::open_initialized(&state.db_path)?;
    store::load_profiles(&conn)
}

#[tauri::command]
pub(crate) fn get_navigation_model(state: State<'_, AppState>) -> AppResult<NavigationModel> {
    let _guard = state.lock.lock().map_err(|error| error.to_string())?;
    let conn = store::open_initialized(&state.db_path)?;
    store::load_navigation_model(&conn)
}

#[tauri::command]
pub(crate) fn update_navigation_model(
    state: State<'_, AppState>,
    model: NavigationModel,
) -> AppResult<NavigationModel> {
    let _guard = state.lock.lock().map_err(|error| error.to_string())?;
    let conn = store::open_initialized(&state.db_path)?;
    store::save_navigation_model(&conn, &model)?;
    store::load_navigation_model(&conn)
}

#[tauri::command]
pub(crate) fn list_app_shortcuts(state: State<'_, AppState>) -> AppResult<Vec<AppShortcut>> {
    let _guard = state.lock.lock().map_err(|error| error.to_string())?;
    let conn = store::open_initialized(&state.db_path)?;
    store::load_app_shortcuts(&conn)
}

#[tauri::command]
pub(crate) fn list_app_shortcut_settings(
    state: State<'_, AppState>,
) -> AppResult<Vec<AppShortcut>> {
    let _guard = state.lock.lock().map_err(|error| error.to_string())?;
    let conn = store::open_initialized(&state.db_path)?;
    store::load_app_shortcut_settings(&conn)
}

#[tauri::command]
pub(crate) fn update_app_shortcuts(
    state: State<'_, AppState>,
    shortcuts: Vec<AppShortcut>,
) -> AppResult<Vec<AppShortcut>> {
    let _guard = state.lock.lock().map_err(|error| error.to_string())?;
    let conn = store::open_initialized(&state.db_path)?;
    store::save_app_shortcuts(&conn, &shortcuts)?;
    store::load_app_shortcut_settings(&conn)
}

#[tauri::command]
pub(crate) fn list_asset_mounts(
    state: State<'_, AppState>,
    asset_id: Option<String>,
) -> AppResult<Vec<AssetMount>> {
    let _guard = state.lock.lock().map_err(|error| error.to_string())?;
    let conn = store::open_initialized(&state.db_path)?;
    store::load_asset_mounts(&conn, asset_id.as_deref())
}

#[tauri::command]
pub(crate) fn list_asset_mount_statuses(
    state: State<'_, AppState>,
    asset_id: Option<String>,
) -> AppResult<Vec<AssetMountStatus>> {
    let _guard = state.lock.lock().map_err(|error| error.to_string())?;
    let conn = store::open_initialized(&state.db_path)?;
    let assets = store::load_assets(&conn)?;
    let profiles = store::load_profiles(&conn)?;
    let mut statuses = Vec::new();

    for asset in assets.iter().filter(|asset| {
        asset_id
            .as_ref()
            .map_or(true, |requested| requested == &asset.id)
    }) {
        for profile in &profiles {
            let inspection = targeting::inspect_mount(profile, asset)?;
            statuses.push(AssetMountStatus {
                asset_id: asset.id.clone(),
                profile_id: profile.id.clone(),
                target_dir: inspection.target_dir,
                target_path: inspection.target_path,
                state: PhysicalMountStateDto::from(inspection.state),
                linked_source: inspection.linked_source,
            });
        }
    }

    Ok(statuses)
}

#[tauri::command]
pub(crate) fn toggle_asset_mount(
    state: State<'_, AppState>,
    asset_id: String,
    profile_id: String,
) -> AppResult<AssetMount> {
    let _guard = state.lock.lock().map_err(|error| error.to_string())?;
    let conn = store::open_initialized(&state.db_path)?;
    let default_strategy = validate_mount_target(&conn, &asset_id, &profile_id)?;
    store::toggle_asset_mount(&conn, &asset_id, &profile_id, default_strategy)
}

#[tauri::command]
pub(crate) fn set_asset_mount(
    state: State<'_, AppState>,
    asset_id: String,
    profile_id: String,
    enabled: bool,
    strategy: Option<DeploymentStrategy>,
) -> AppResult<AssetMount> {
    let _guard = state.lock.lock().map_err(|error| error.to_string())?;
    let conn = store::open_initialized(&state.db_path)?;
    let default_strategy = validate_mount_target(&conn, &asset_id, &profile_id)?;
    store::set_asset_mount(
        &conn,
        &asset_id,
        &profile_id,
        enabled,
        strategy.unwrap_or(default_strategy),
    )
}

#[tauri::command]
pub(crate) fn scan_sources(state: State<'_, AppState>) -> AppResult<Vec<Asset>> {
    let _guard = state.lock.lock().map_err(|error| error.to_string())?;
    let conn = store::open_initialized(&state.db_path)?;
    refresh_all_sources(&conn)
}

#[tauri::command]
pub(crate) fn scan_skill_sources(state: State<'_, AppState>) -> AppResult<Vec<Asset>> {
    let _guard = state.lock.lock().map_err(|error| error.to_string())?;
    let conn = store::open_initialized(&state.db_path)?;
    let sources = store::load_skill_sources(&conn)?;
    scan_selected_sources(&conn, sources, scanner::scan_skill_source)
}

#[tauri::command]
pub(crate) fn adopt_app_local_skill(
    state: State<'_, AppState>,
    asset_id: String,
) -> AppResult<Asset> {
    let _guard = state.lock.lock().map_err(|error| error.to_string())?;
    let conn = store::open_initialized(&state.db_path)?;
    let assets = store::load_assets(&conn)?;
    let asset = assets
        .iter()
        .find(|candidate| candidate.id == asset_id)
        .ok_or_else(|| format!("asset not found: {asset_id}"))?;
    if !matches!(asset.kind, AssetKind::Skill) {
        return Err("only skill assets can be adopted".to_string());
    }

    let source = store::load_sources(&conn)?
        .into_iter()
        .find(|candidate| candidate.id == asset.source_id)
        .ok_or_else(|| format!("source not found: {}", asset.source_id))?;
    if !matches!(
        source.source_origin,
        SourceOrigin::AppTarget | SourceOrigin::AppLocal
    ) {
        return Err("only app-local skill assets need adoption".to_string());
    }

    let origin_bucket = source
        .origin_app_kind
        .map(|kind| format!("{kind:?}").to_ascii_lowercase())
        .unwrap_or_else(|| source.id.clone());
    let library_root = app_library_skill_root()?;
    let target_dir = library_root.join(origin_bucket).join(&asset.name);
    if target_dir.exists() {
        return Err(format!(
            "adopted skill already exists: {}",
            target_dir.display()
        ));
    }
    copy_dir(Path::new(&asset.absolute_path), &target_dir)?;

    let library_source = assetiweave_library_source();
    store::upsert_source(&conn, &library_source)?;
    let library_assets = scanner::scan_skill_source(&library_source)?;
    store::replace_source_assets(&conn, &library_source.id, &library_assets)?;
    library_assets
        .into_iter()
        .find(|candidate| candidate.absolute_path == target_dir.to_string_lossy())
        .ok_or_else(|| "adopted skill was copied but not found during rescan".to_string())
}

fn scan_selected_sources(
    conn: &rusqlite::Connection,
    sources: Vec<Source>,
    scan: fn(&Source) -> AppResult<Vec<Asset>>,
) -> AppResult<Vec<Asset>> {
    for mut source in prune_missing_sources(conn, sources)? {
        if !source.enabled {
            continue;
        }

        let now = Utc::now().to_rfc3339();
        match scan(&source) {
            Ok(assets) => {
                store::replace_source_assets(&conn, &source.id, &assets)?;
                source.last_scanned_at = Some(now);
                source.last_scan_status = Some(format!("ok: {} assets", assets.len()));
                store::upsert_source(&conn, &source)?;
            }
            Err(error) => {
                if should_remove_source_on_scan_error(&error) {
                    store::delete_source(conn, &source.id)?;
                    continue;
                }
                source.last_scanned_at = Some(now);
                source.last_scan_status = Some(format!("error: {error}"));
                store::upsert_source(&conn, &source)?;
            }
        }
    }

    cleanup_orphan_asset_records(conn)?;
    store::load_assets(&conn)
}

pub(crate) fn refresh_all_sources(conn: &rusqlite::Connection) -> AppResult<Vec<Asset>> {
    let sources = store::load_sources(conn)?;
    scan_selected_sources(conn, sources, scanner::scan_source)
}

pub(crate) fn refresh_recorded_assets(conn: &rusqlite::Connection) -> AppResult<Vec<Asset>> {
    let sources = prune_missing_sources(conn, store::load_sources(conn)?)?;
    let source_map: HashMap<&str, &Source> = sources
        .iter()
        .map(|source| (source.id.as_str(), source))
        .collect();
    let mut assets_by_source: HashMap<String, Vec<Asset>> = sources
        .iter()
        .map(|source| (source.id.clone(), Vec::new()))
        .collect();
    let mut removed_by_source: HashMap<String, usize> = HashMap::new();
    let mut updated_by_source: HashMap<String, usize> = HashMap::new();
    let mut orphan_source_ids = Vec::new();
    let now = Utc::now().to_rfc3339();

    for asset in store::load_assets(conn)? {
        let Some(source) = source_map.get(asset.source_id.as_str()) else {
            orphan_source_ids.push(asset.source_id.clone());
            continue;
        };

        match scanner::refresh_recorded_asset(source, &asset, &now) {
            Ok(Some(refreshed)) => {
                if refreshed.content_hash != asset.content_hash
                    || refreshed.description != asset.description
                {
                    *updated_by_source.entry(source.id.clone()).or_default() += 1;
                }
                assets_by_source
                    .entry(source.id.clone())
                    .or_default()
                    .push(refreshed);
            }
            Ok(None) => {
                *removed_by_source.entry(source.id.clone()).or_default() += 1;
            }
            Err(_) => {
                assets_by_source
                    .entry(source.id.clone())
                    .or_default()
                    .push(asset);
            }
        }
    }

    for source in sources {
        let retained_assets = assets_by_source.remove(&source.id).unwrap_or_default();
        let retained_count = retained_assets.len();
        store::replace_source_assets(conn, &source.id, &retained_assets)?;

        let removed_count = removed_by_source.get(&source.id).copied().unwrap_or(0);
        let updated_count = updated_by_source.get(&source.id).copied().unwrap_or(0);
        let mut source = source;
        source.last_scanned_at = Some(now.clone());
        source.last_scan_status = Some(format!(
            "validated: {retained_count} assets, {removed_count} removed, {updated_count} updated"
        ));
        store::upsert_source(conn, &source)?;
    }

    orphan_source_ids.sort();
    orphan_source_ids.dedup();
    for source_id in orphan_source_ids {
        store::replace_source_assets(conn, &source_id, &[])?;
    }

    cleanup_orphan_asset_records(conn)?;
    store::load_assets(conn)
}

fn cleanup_orphan_asset_records(conn: &rusqlite::Connection) -> AppResult<()> {
    store::delete_orphan_asset_mounts(conn)?;
    store::delete_orphan_deployment_state(conn)?;
    Ok(())
}

fn prune_missing_sources(
    conn: &rusqlite::Connection,
    sources: Vec<Source>,
) -> AppResult<Vec<Source>> {
    let mut retained_sources = Vec::new();
    for source in sources {
        if source_root_is_missing(&source) {
            store::delete_source(conn, &source.id)?;
        } else {
            retained_sources.push(source);
        }
    }
    Ok(retained_sources)
}

fn source_root_is_missing(source: &Source) -> bool {
    expand_path(&source.root_path)
        .map(|root| !root.exists())
        .unwrap_or(false)
}

fn should_remove_source_on_scan_error(error: &str) -> bool {
    error.starts_with("source path does not exist:")
}

#[tauri::command]
pub(crate) fn create_plan(
    state: State<'_, AppState>,
    profile_id: Option<String>,
) -> AppResult<DeploymentPlan> {
    let _guard = state.lock.lock().map_err(|error| error.to_string())?;
    let conn = store::open_initialized(&state.db_path)?;
    let assets = store::load_assets(&conn)?;
    let profiles = store::load_profiles(&conn)?;
    let mounts = store::load_enabled_asset_mounts(&conn, profile_id.as_deref())?;
    Ok(planner::build_plan(
        &assets,
        &profiles,
        &mounts,
        profile_id.as_deref(),
    ))
}

#[tauri::command]
pub(crate) fn execute_plan(
    state: State<'_, AppState>,
    plan: DeploymentPlan,
    action_ids: Option<Vec<String>>,
) -> AppResult<ExecutionResult> {
    let _guard = state.lock.lock().map_err(|error| error.to_string())?;
    let conn = store::open_initialized(&state.db_path)?;
    let profiles = store::load_profiles(&conn)?;
    let assets = store::load_assets(&conn)?;
    executor::execute_deployment_plan(&conn, &profiles, &assets, &plan, action_ids.as_deref())
}

#[tauri::command]
pub(crate) fn reveal_path(path: String) -> AppResult<()> {
    platform::reveal_path(path)
}

fn validate_mount_target(
    conn: &rusqlite::Connection,
    asset_id: &str,
    profile_id: &str,
) -> AppResult<DeploymentStrategy> {
    let asset = store::load_assets(conn)?
        .iter()
        .find(|asset| asset.id == asset_id)
        .cloned()
        .ok_or_else(|| format!("asset not found: {asset_id}"))?;
    let source = store::load_sources(conn)?
        .into_iter()
        .find(|source| source.id == asset.source_id)
        .ok_or_else(|| format!("source not found: {}", asset.source_id))?;
    if matches!(
        source.source_origin,
        SourceOrigin::AppTarget | SourceOrigin::AppLocal
    ) {
        return Err(
            "app-local skills must be adopted into the AssetIWeave library before mounting"
                .to_string(),
        );
    }

    let profile = store::load_profiles(conn)?
        .into_iter()
        .find(|profile| profile.id == profile_id)
        .ok_or_else(|| format!("profile not found: {profile_id}"))?;

    Ok(profile.deployment_strategy)
}

fn assetiweave_library_source() -> Source {
    Source {
        id: "assetiweave-library-skills".to_string(),
        name: "AssetIWeave Library Skills".to_string(),
        kind: SourceKind::Local,
        root_path: "~/.assetiweave/library/skills".to_string(),
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

fn copy_dir(source: &Path, target: &Path) -> AppResult<()> {
    for entry in WalkDir::new(source).into_iter().filter_map(Result::ok) {
        let relative = entry
            .path()
            .strip_prefix(source)
            .map_err(|error| error.to_string())?;
        let destination = target.join(relative);
        if entry.file_type().is_dir() {
            fs::create_dir_all(&destination).map_err(|error| error.to_string())?;
        } else if entry.file_type().is_file() {
            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent).map_err(|error| error.to_string())?;
            }
            fs::copy(entry.path(), destination).map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use assetiweave_core::{AssetFormat, AssetKind, DeploymentStrategy, SourceKind};
    use std::path::PathBuf;

    #[test]
    fn refresh_recorded_assets_prunes_missing_sources() {
        let db_path = unique_temp_path("assetiweave-refresh-recorded");
        let conn = store::open_initialized(&db_path).expect("open initialized db");
        let source = test_missing_source("missing-recorded-source");
        store::upsert_source(&conn, &source).expect("insert source");

        refresh_recorded_assets(&conn).expect("refresh recorded assets");

        assert!(!store::load_sources(&conn)
            .expect("load sources")
            .iter()
            .any(|candidate| candidate.id == source.id));
        std::fs::remove_file(db_path).ok();
    }

    #[test]
    fn source_scan_prunes_missing_sources_without_error_row() {
        let db_path = unique_temp_path("assetiweave-scan-missing-source");
        let conn = store::open_initialized(&db_path).expect("open initialized db");
        let source = test_missing_source("missing-scan-source");
        store::upsert_source(&conn, &source).expect("insert source");

        scan_selected_sources(&conn, vec![source.clone()], scanner::scan_source)
            .expect("scan selected sources");

        assert!(!store::load_sources(&conn)
            .expect("load sources")
            .iter()
            .any(|candidate| candidate.id == source.id));
        std::fs::remove_file(db_path).ok();
    }

    #[test]
    fn refresh_recorded_assets_removes_mounts_for_deleted_assets() {
        let db_path = unique_temp_path("assetiweave-refresh-deleted-mount");
        let source_root = unique_temp_path("assetiweave-existing-source");
        std::fs::create_dir_all(&source_root).expect("create source root");
        let conn = store::open_initialized(&db_path).expect("open initialized db");
        let source = test_source("source-with-deleted-asset", source_root.clone());
        let asset = test_asset(&source, "deleted-asset", source_root.join("deleted-asset"));
        store::upsert_source(&conn, &source).expect("insert source");
        store::replace_source_assets(&conn, &source.id, std::slice::from_ref(&asset))
            .expect("insert asset");
        store::set_asset_mount(
            &conn,
            &asset.id,
            "codex",
            true,
            DeploymentStrategy::SymlinkToSource,
        )
        .expect("insert mount");

        refresh_recorded_assets(&conn).expect("refresh recorded assets");

        assert!(store::load_assets(&conn)
            .expect("load assets")
            .iter()
            .all(|candidate| candidate.id != asset.id));
        assert!(store::load_asset_mounts(&conn, Some(&asset.id))
            .expect("load mounts")
            .is_empty());
        std::fs::remove_dir_all(source_root).ok();
        std::fs::remove_file(db_path).ok();
    }

    fn test_missing_source(id: &str) -> Source {
        let root_path = unique_temp_path(id);
        test_source(id, root_path)
    }

    fn test_source(id: &str, root_path: PathBuf) -> Source {
        Source {
            id: id.to_string(),
            name: id.to_string(),
            kind: SourceKind::Local,
            root_path: root_path.to_string_lossy().to_string(),
            scanner_kind: SourceScannerKind::Skill,
            source_origin: SourceOrigin::GitRepo,
            repo_root: None,
            scan_root: String::new(),
            origin_app_kind: None,
            include_globs: vec!["**/SKILL.md".to_string()],
            exclude_globs: vec![],
            default_kind: Some(AssetKind::Skill),
            enabled: true,
            priority: 0,
            last_scanned_at: None,
            last_scan_status: None,
        }
    }

    fn test_asset(source: &Source, id: &str, absolute_path: PathBuf) -> Asset {
        Asset {
            id: id.to_string(),
            source_id: source.id.clone(),
            name: id.to_string(),
            kind: AssetKind::Skill,
            format: AssetFormat::Directory,
            relative_path: id.to_string(),
            absolute_path: absolute_path.to_string_lossy().to_string(),
            entry_file: None,
            description: None,
            content_hash: None,
            discovered_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
        }
    }

    fn unique_temp_path(prefix: &str) -> PathBuf {
        std::env::temp_dir().join(format!("{prefix}-{}", Uuid::new_v4()))
    }
}
