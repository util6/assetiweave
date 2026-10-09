use tauri::State;

use crate::adapters::app_state::AppState;
use crate::backend::application::prelude::*;
use crate::backend::application::AppResult as RuntimeAppResult;
use crate::backend::domain::{Asset, AssetKind, CatalogAsset};

#[tauri::command]
pub(crate) async fn list_assets(
    state: State<'_, AppState>,
    kind: Option<AssetKind>,
) -> RuntimeAppResult<Vec<CatalogAsset>> {
    AppService::from_runtime(&state.runtime)
        .list_assets(ListAssetsParams { kind })
        .await
}

#[tauri::command]
pub(crate) async fn list_source_assets(
    state: State<'_, AppState>,
    kind: Option<AssetKind>,
) -> RuntimeAppResult<Vec<CatalogAsset>> {
    AppService::from_runtime(&state.runtime)
        .list_source_assets(kind)
        .await
}

#[tauri::command]
pub(crate) async fn update_asset_description(
    state: State<'_, AppState>,
    asset_id: String,
    description: Option<String>,
) -> RuntimeAppResult<Asset> {
    let result = AppService::from_runtime(&state.runtime)
        .update_asset_description(asset_id.clone(), description)
        .await;

    match &result {
        Ok(asset) => tracing::info!(
            action = "asset.update_description",
            asset_id = %asset.id,
            asset_name = %asset.name,
            asset_kind = ?asset.kind,
            "更新资产说明成功"
        ),
        Err(error) => tracing::error!(
            action = "asset.update_description",
            asset_id = %asset_id,
            error = %error,
            "更新资产说明失败"
        ),
    }
    result
}

#[tauri::command]
pub(crate) async fn delete_asset(
    state: State<'_, AppState>,
    asset_id: String,
    unmount: Option<bool>,
) -> RuntimeAppResult<Asset> {
    let result = AppService::from_runtime(&state.runtime)
        .delete_asset(asset_id.clone(), unmount.unwrap_or(false))
        .await;

    match &result {
        Ok(asset) => tracing::info!(
            action = "asset.delete",
            asset_id = %asset.id,
            asset_name = %asset.name,
            asset_kind = ?asset.kind,
            "删除资产成功"
        ),
        Err(error) => tracing::error!(
            action = "asset.delete",
            asset_id = %asset_id,
            error = %error,
            "删除资产失败"
        ),
    }
    result
}
