mod commands;
mod defaults;
mod executor;
mod path_utils;
mod planner;
mod platform;
mod scanner;
mod store;
mod types;

use crate::{
    commands::{
        create_plan, create_source, delete_source, execute_plan, get_app_overview,
        get_navigation_model, list_assets, list_profiles, list_sources, reveal_path, scan_sources,
        update_source,
    },
    path_utils::app_db_path,
    store::open_initialized,
    types::AppState,
};
use std::sync::Mutex;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let db_path = app_db_path().expect("failed to resolve AssetIWeave database path");
    open_initialized(&db_path).expect("failed to initialize AssetIWeave database");

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(AppState {
            db_path,
            lock: Mutex::new(()),
        })
        .invoke_handler(tauri::generate_handler![
            get_app_overview,
            list_assets,
            list_sources,
            create_source,
            update_source,
            delete_source,
            list_profiles,
            get_navigation_model,
            scan_sources,
            create_plan,
            execute_plan,
            reveal_path
        ])
        .run(tauri::generate_context!())
        .expect("error while running AssetIWeave");
}
