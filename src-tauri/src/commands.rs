use crate::{
    executor, planner, platform, scanner, store,
    types::{AppOverview, AppResult, AppState, ExecutionResult, SourceInput},
};
use assetiweave_core::{Asset, DeploymentPlan, Source, TargetProfile};
use chrono::Utc;
use tauri::State;
use uuid::Uuid;

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
pub(crate) fn create_source(state: State<'_, AppState>, source: SourceInput) -> AppResult<Source> {
    let _guard = state.lock.lock().map_err(|error| error.to_string())?;
    let conn = store::open_initialized(&state.db_path)?;
    let source = Source {
        id: source.id.unwrap_or_else(|| Uuid::new_v4().to_string()),
        name: source.name,
        kind: source.kind,
        root_path: source.root_path,
        include_globs: source.include_globs,
        exclude_globs: source.exclude_globs,
        default_kind: source.default_kind,
        enabled: source.enabled,
        priority: source.priority,
        last_scanned_at: None,
        last_scan_status: Some("pending".to_string()),
    };
    store::upsert_source(&conn, &source)?;
    Ok(source)
}

#[tauri::command]
pub(crate) fn update_source(state: State<'_, AppState>, source: Source) -> AppResult<Source> {
    let _guard = state.lock.lock().map_err(|error| error.to_string())?;
    let conn = store::open_initialized(&state.db_path)?;
    store::upsert_source(&conn, &source)?;
    Ok(source)
}

#[tauri::command]
pub(crate) fn delete_source(state: State<'_, AppState>, id: String) -> AppResult<()> {
    let _guard = state.lock.lock().map_err(|error| error.to_string())?;
    let conn = store::open_initialized(&state.db_path)?;
    store::delete_source(&conn, &id)
}

#[tauri::command]
pub(crate) fn list_profiles(state: State<'_, AppState>) -> AppResult<Vec<TargetProfile>> {
    let _guard = state.lock.lock().map_err(|error| error.to_string())?;
    let conn = store::open_initialized(&state.db_path)?;
    store::load_profiles(&conn)
}

#[tauri::command]
pub(crate) fn scan_sources(state: State<'_, AppState>) -> AppResult<Vec<Asset>> {
    let _guard = state.lock.lock().map_err(|error| error.to_string())?;
    let conn = store::open_initialized(&state.db_path)?;
    let sources = store::load_sources(&conn)?;

    for mut source in sources {
        if !source.enabled {
            continue;
        }

        let now = Utc::now().to_rfc3339();
        match scanner::scan_source(&source) {
            Ok(assets) => {
                store::replace_source_assets(&conn, &source.id, &assets)?;
                source.last_scanned_at = Some(now);
                source.last_scan_status = Some(format!("ok: {} assets", assets.len()));
                store::upsert_source(&conn, &source)?;
            }
            Err(error) => {
                source.last_scanned_at = Some(now);
                source.last_scan_status = Some(format!("error: {error}"));
                store::upsert_source(&conn, &source)?;
            }
        }
    }

    store::load_assets(&conn)
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
    Ok(planner::build_plan(
        &assets,
        &profiles,
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
