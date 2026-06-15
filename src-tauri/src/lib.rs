mod app_settings;
mod background_tasks;
mod command_registry;
mod commands;
mod conversations;
mod defaults;
mod engine;
mod executor;
mod logs;
mod models;
mod path_utils;
mod planner;
mod platform;
mod policy;
mod protocol;
mod runtime;
mod scanner;
mod service;
mod store;
mod targeting;
mod types;

use crate::{
    commands::{
        acquire_skill, apply_skill_group_exclusive_mount, apply_skill_group_mount, backup_skill,
        check_skill_remote_sources, create_plan, create_profile, create_skill_group, create_source,
        delete_asset, delete_profile, delete_skill_group, delete_source,
        disable_conversation_source, execute_plan, export_conversation_session,
        export_web_record_session, get_app_overview, get_app_settings, get_conversation_question,
        get_conversation_session, get_conversation_sync_task, get_navigation_model,
        get_skill_backup_settings, get_web_record_session, list_app_shortcut_settings,
        list_app_shortcuts, list_asset_mount_statuses, list_asset_mounts, list_assets,
        list_conversation_adapters, list_conversation_questions, list_conversation_sessions,
        list_conversation_sources, list_profiles, list_skill_groups, list_skill_remote_sources,
        list_skill_sources, list_sources, list_web_record_sessions, merge_conversation_questions,
        mount_asset_mount, preview_skill_group_exclusive_mount, refresh_asset_mount_statuses,
        register_conversation_adapter, reveal_path, save_app_settings,
        scaffold_conversation_adapter, scan_skill_sources, scan_sources, search_skills,
        set_asset_mount, set_skill_group_manual_members, split_conversation_question,
        sync_conversations, toggle_asset_mount, try_run_conversation_adapter, unmount_asset_mount,
        unregister_conversation_adapter, update_app_shortcuts, update_asset_description,
        update_navigation_model, update_profile, update_skill_backup_settings, update_skill_group,
        update_source, upsert_conversation_source, validate_conversation_adapter,
    },
    logs::{logs_get_snapshot, logs_open_log_directory, logs_write_operation},
    path_utils::app_db_path,
    store::open_initialized,
    types::AppState,
};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use tauri::Manager;
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};

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
    let window_shutdown_db_path = db_path.clone();
    let app_shutdown_db_path = db_path.clone();

    let app = tauri::Builder::default()
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
        .on_window_event(move |window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let state = window.state::<AppState>();
                if state.allow_close.swap(false, Ordering::SeqCst) {
                    return;
                }
                if state.background_tasks.has_running_tasks() {
                    api.prevent_close();
                    if state.exit_prompt_open.swap(true, Ordering::SeqCst) {
                        return;
                    }

                    let prompt_window = window.clone();
                    let close_window = window.clone();
                    let allow_close = state.allow_close.clone();
                    let allow_exit = state.allow_exit.clone();
                    let exit_prompt_open = state.exit_prompt_open.clone();
                    prompt_window
                        .dialog()
                        .message(
                            "对话记录仍在后台同步。现在退出会中断任务，未完成的写入将回滚。\n\nA conversation sync is still running. Quitting now will interrupt it.",
                        )
                        .title("后台任务仍在运行 / Background task running")
                        .kind(MessageDialogKind::Warning)
                        .buttons(MessageDialogButtons::OkCancelCustom(
                            "仍然退出 / Quit anyway".to_string(),
                            "继续等待 / Keep waiting".to_string(),
                        ))
                        .show(move |quit_anyway| {
                            exit_prompt_open.store(false, Ordering::SeqCst);
                            if quit_anyway {
                                allow_close.store(true, Ordering::SeqCst);
                                allow_exit.store(true, Ordering::SeqCst);
                                if let Err(error) = close_window.close() {
                                    eprintln!("failed to close AssetIWeave after confirmation: {error}");
                                }
                            }
                        });
                    return;
                }

                sync_before_close(&window_shutdown_db_path);
            }
        })
        .manage(AppState {
            db_path,
            lock: Arc::new(Mutex::new(())),
            background_tasks: Arc::new(background_tasks::BackgroundTaskRegistry::default()),
            allow_close: Arc::new(AtomicBool::new(false)),
            allow_exit: Arc::new(AtomicBool::new(false)),
            exit_prompt_open: Arc::new(AtomicBool::new(false)),
        })
        .invoke_handler(tauri::generate_handler![
            get_app_overview,
            get_app_settings,
            save_app_settings,
            list_assets,
            get_skill_backup_settings,
            update_skill_backup_settings,
            backup_skill,
            search_skills,
            acquire_skill,
            list_skill_remote_sources,
            check_skill_remote_sources,
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
            list_conversation_adapters,
            scaffold_conversation_adapter,
            validate_conversation_adapter,
            register_conversation_adapter,
            unregister_conversation_adapter,
            try_run_conversation_adapter,
            list_conversation_sources,
            upsert_conversation_source,
            disable_conversation_source,
            sync_conversations,
            get_conversation_sync_task,
            list_conversation_sessions,
            get_conversation_session,
            export_conversation_session,
            list_web_record_sessions,
            get_web_record_session,
            export_web_record_session,
            list_conversation_questions,
            get_conversation_question,
            merge_conversation_questions,
            split_conversation_question,
            create_plan,
            execute_plan,
            logs_get_snapshot,
            logs_open_log_directory,
            logs_write_operation,
            reveal_path
        ])
        .build(tauri::generate_context!())
        .expect("error while running AssetIWeave");
    app.run(move |app_handle, event| {
        if let tauri::RunEvent::ExitRequested { api, .. } = event {
            let state = app_handle.state::<AppState>();
            if state.allow_exit.swap(false, Ordering::SeqCst) {
                return;
            }
            if state.background_tasks.has_running_tasks() {
                api.prevent_exit();
                if state.exit_prompt_open.swap(true, Ordering::SeqCst) {
                    return;
                }

                let prompt_app = app_handle.clone();
                let exit_app = app_handle.clone();
                let allow_exit = state.allow_exit.clone();
                let exit_prompt_open = state.exit_prompt_open.clone();
                prompt_app
                    .dialog()
                    .message(
                        "对话记录仍在后台同步。现在退出会中断任务，当前未完成的写入将回滚。\n\nA conversation sync is still running. Quitting now will interrupt it.",
                    )
                    .title("后台任务仍在运行 / Background task running")
                    .kind(MessageDialogKind::Warning)
                    .buttons(MessageDialogButtons::OkCancelCustom(
                        "仍然退出 / Quit anyway".to_string(),
                        "继续等待 / Keep waiting".to_string(),
                    ))
                    .show(move |quit_anyway| {
                        exit_prompt_open.store(false, Ordering::SeqCst);
                        if quit_anyway {
                            allow_exit.store(true, Ordering::SeqCst);
                            exit_app.exit(0);
                        }
                    });
                return;
            }

            sync_before_close(&app_shutdown_db_path);
        }
    });
}

fn sync_before_close(db_path: &std::path::Path) {
    match open_initialized(db_path) {
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

pub fn run_engine_stdio() {
    if let Err(error) = engine::run_stdio() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
