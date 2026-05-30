mod commands;
mod defaults;
mod executor;
mod path_utils;
mod planner;
mod platform;
mod scanner;
mod store;
mod targeting;
mod types;

use crate::{
    commands::{
        adopt_app_local_skill, create_plan, create_source, delete_source, execute_plan,
        get_app_overview, get_navigation_model, list_app_shortcut_settings, list_app_shortcuts,
        list_asset_mount_statuses, list_asset_mounts, list_assets, list_profiles,
        list_skill_sources, list_sources, reveal_path, scan_skill_sources, scan_sources,
        set_asset_mount, toggle_asset_mount, unmount_asset_mount, update_app_shortcuts,
        update_navigation_model, update_source,
    },
    path_utils::app_db_path,
    store::open_initialized,
    types::AppState,
};
use std::sync::Mutex;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let db_path = app_db_path().expect("failed to resolve AssetIWeave database path");
    {
        let conn = open_initialized(&db_path).expect("failed to initialize AssetIWeave database");
        if let Err(error) = commands::refresh_recorded_assets(&conn) {
            eprintln!("failed to validate recorded AssetIWeave assets on startup: {error}");
        }
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(AppState {
            db_path,
            lock: Mutex::new(()),
        })
        .invoke_handler(tauri::generate_handler![
            get_app_overview,
            list_assets,
            list_sources,
            list_skill_sources,
            create_source,
            update_source,
            delete_source,
            list_profiles,
            get_navigation_model,
            update_navigation_model,
            list_app_shortcuts,
            list_app_shortcut_settings,
            update_app_shortcuts,
            list_asset_mounts,
            list_asset_mount_statuses,
            toggle_asset_mount,
            unmount_asset_mount,
            set_asset_mount,
            scan_sources,
            scan_skill_sources,
            adopt_app_local_skill,
            create_plan,
            execute_plan,
            reveal_path
        ])
        .run(tauri::generate_context!())
        .expect("error while running AssetIWeave");
}
