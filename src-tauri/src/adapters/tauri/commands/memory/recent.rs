use tauri::State;

use crate::adapters::app_state::AppState;
use crate::backend::application::prelude::*;
use crate::backend::application::AppResult as RuntimeAppResult;

#[tauri::command]
pub(crate) async fn get_memory_recent_snapshot(
    state: State<'_, AppState>,
) -> RuntimeAppResult<crate::backend::domain::RecentMemoryStateView> {
    AppService::from_runtime(&state.runtime)
        .get_recent_memory_snapshot()
        .await
        .into()
}

#[tauri::command]
pub(crate) async fn duplicate_memory_generation_skill(
    state: State<'_, AppState>,
) -> RuntimeAppResult<crate::backend::domain::CatalogAsset> {
    AppService::from_runtime(&state.runtime)
        .duplicate_generation_skill_to_library()
        .await
        .into()
}

#[tauri::command]
pub(crate) async fn reset_memory_generation_skill_to_default(
    state: State<'_, AppState>,
) -> RuntimeAppResult<()> {
    AppService::from_runtime(&state.runtime)
        .reset_generation_skill_to_default()
        .await
        .into()
}
