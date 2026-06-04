mod commands;
mod defaults;
mod engine;
mod executor;
mod logs;
mod path_utils;
mod planner;
mod platform;
mod scanner;
mod service;
mod store;
mod targeting;
mod types;

use crate::{
    commands::{
        apply_skill_group_exclusive_mount, apply_skill_group_mount, backup_skill, create_plan,
        create_profile, create_skill_group, create_source, delete_asset, delete_profile,
        delete_skill_group, delete_source, execute_plan, get_app_overview, get_navigation_model,
        get_skill_backup_settings, list_app_shortcut_settings, list_app_shortcuts,
        list_asset_mount_statuses, list_asset_mounts, list_assets, list_profiles,
        list_skill_groups, list_skill_sources, list_sources, mount_asset_mount,
        preview_skill_group_exclusive_mount, refresh_asset_mount_statuses, reveal_path,
        scan_skill_sources, scan_sources, set_asset_mount, set_skill_group_manual_members,
        toggle_asset_mount, unmount_asset_mount, update_app_shortcuts, update_asset_description,
        update_navigation_model, update_profile, update_skill_backup_settings, update_skill_group,
        update_source,
    },
    logs::{logs_get_snapshot, logs_open_log_directory, logs_write_operation},
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
        if let Err(error) = commands::sync_asset_mount_observations(&conn, None) {
            eprintln!("failed to sync AssetIWeave mount observations on startup: {error}");
        }
    }
    if let Err(error) = logs::write_startup_log() {
        eprintln!("failed to write AssetIWeave startup log: {error}");
    }
    let shutdown_db_path = db_path.clone();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_process::init())
        .setup(|app| {
            #[cfg(desktop)]
            {
                app.handle()
                    .plugin(tauri_plugin_updater::Builder::new().build())?;
            }
            Ok(())
        })
        .on_window_event(move |_window, event| {
            if matches!(event, tauri::WindowEvent::CloseRequested { .. }) {
                match open_initialized(&shutdown_db_path) {
                    Ok(conn) => {
                        if let Err(error) = commands::sync_asset_mount_observations(&conn, None) {
                            eprintln!("failed to sync AssetIWeave mount observations before close: {error}");
                        }
                    }
                    Err(error) => {
                        eprintln!("failed to open AssetIWeave database before close: {error}");
                    }
                }
            }
        })
        .manage(AppState {
            db_path,
            lock: Mutex::new(()),
        })
        .invoke_handler(tauri::generate_handler![
            get_app_overview,
            list_assets,
            get_skill_backup_settings,
            update_skill_backup_settings,
            backup_skill,
            list_sources,
            list_skill_sources,
            create_source,
            update_source,
            delete_source,
            update_asset_description,
            delete_asset,
            list_profiles,
            create_profile,
            update_profile,
            delete_profile,
            get_navigation_model,
            update_navigation_model,
            list_app_shortcuts,
            list_app_shortcut_settings,
            update_app_shortcuts,
            list_asset_mounts,
            list_asset_mount_statuses,
            refresh_asset_mount_statuses,
            list_skill_groups,
            create_skill_group,
            update_skill_group,
            delete_skill_group,
            set_skill_group_manual_members,
            apply_skill_group_mount,
            preview_skill_group_exclusive_mount,
            apply_skill_group_exclusive_mount,
            toggle_asset_mount,
            mount_asset_mount,
            unmount_asset_mount,
            set_asset_mount,
            scan_sources,
            scan_skill_sources,
            create_plan,
            execute_plan,
            logs_get_snapshot,
            logs_open_log_directory,
            logs_write_operation,
            reveal_path
        ])
        .run(tauri::generate_context!())
        .expect("error while running AssetIWeave");
}

pub fn run_engine_stdio() {
    if let Err(error) = engine::run_stdio() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
