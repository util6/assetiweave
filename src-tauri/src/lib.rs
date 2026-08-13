mod adapters;
mod backend;

use crate::{
    adapters::{app_state::AppState, tauri::background_tasks::BackgroundTaskRegistry},
    backend::{
        application::AppService, data_backup::backup_database_from_settings,
        logs::write_startup_log, path_utils::app_db_path,
    },
};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex,
};
use tauri::{Emitter, Manager};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};

const APP_CLOSE_REQUESTED_EVENT: &str = "app-close-requested";

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    backend::builtin_skills::install_builtin_skills()
        .expect("failed to install AssetIWeave system Skills");
    let db_path = app_db_path().expect("failed to resolve AssetIWeave database path");
    let conversation_full_sync_on_startup_enabled =
        match backend::app_settings::conversation_full_sync_on_startup_enabled() {
            Ok(enabled) => enabled,
            Err(error) => {
                eprintln!("failed to read Conversation startup sync setting: {error}");
                true
            }
        };
    let conversation_payload_policy_reparse_required = {
        let service = AppService::open_with_db_path(db_path.clone())
            .expect("failed to initialize AssetIWeave database");
        if let Err(error) = service.interrupt_stale_memory_runs() {
            eprintln!("failed to mark interrupted Memory runs on startup: {error}");
        }
        if let Err(error) = service.refresh_recorded_assets() {
            eprintln!("failed to validate recorded AssetIWeave assets on startup: {error}");
        }
        if let Err(error) = service.refresh_asset_mount_statuses(None) {
            eprintln!("failed to sync AssetIWeave mount observations on startup: {error}");
        }
        match service.conversation_payload_policy_reparse_required() {
            Ok(required) => required,
            Err(error) => {
                eprintln!("failed to inspect Conversation payload policy state: {error}");
                false
            }
        }
    };
    if let Err(error) = write_startup_log() {
        eprintln!("failed to write AssetIWeave startup log: {error}");
    }
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
                            "仍有后台任务正在运行。现在退出会中断任务，未完成的写入可能不会保存。\n\nA background task is still running. Quitting now will interrupt it.",
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

                api.prevent_close();
                if state.exit_prompt_open.swap(true, Ordering::SeqCst) {
                    return;
                }
                if let Err(error) = window.emit(APP_CLOSE_REQUESTED_EVENT, ()) {
                    eprintln!("failed to notify frontend about close request: {error}");
                    state.exit_prompt_open.store(false, Ordering::SeqCst);
                    state.allow_close.store(true, Ordering::SeqCst);
                    state.allow_exit.store(true, Ordering::SeqCst);
                    if let Err(close_error) = window.close() {
                        eprintln!("failed to close AssetIWeave after close prompt notification error: {close_error}");
                    }
                }
            }
        })
        .manage(AppState {
            db_path,
            lock: Arc::new(Mutex::new(())),
            background_tasks: Arc::new(BackgroundTaskRegistry::default()),
            allow_close: Arc::new(AtomicBool::new(false)),
            allow_exit: Arc::new(AtomicBool::new(false)),
            exit_prompt_open: Arc::new(AtomicBool::new(false)),
            shutdown_sync_done: Arc::new(AtomicBool::new(false)),
        })
        .invoke_handler(adapters::tauri::command_handler())
        .build(tauri::generate_context!())
        .expect("error while running AssetIWeave");
    if conversation_full_sync_on_startup_enabled && conversation_payload_policy_reparse_required {
        let state = app.state::<AppState>();
        let params = backend::application::ConversationSyncParams {
            source_id: None,
            adapter_id: None,
            record_kind: None,
            mode: backend::application::ConversationSyncMode::Full,
            dry_run: false,
        };
        if let Err(error) = adapters::tauri::commands::start_conversation_sync_background(
            app.handle().clone(),
            state.db_path.clone(),
            state.background_tasks.clone(),
            params,
        ) {
            eprintln!("failed to start Conversation payload policy reparse: {error}");
        }
    }
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
                        "仍有后台任务正在运行。现在退出会中断任务，未完成的写入可能不会保存。\n\nA background task is still running. Quitting now will interrupt it.",
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

            api.prevent_exit();
            if state.exit_prompt_open.swap(true, Ordering::SeqCst) {
                return;
            }
            if let Err(error) = app_handle.emit(APP_CLOSE_REQUESTED_EVENT, ()) {
                eprintln!("failed to notify frontend about app exit request: {error}");
                state.exit_prompt_open.store(false, Ordering::SeqCst);
                state.allow_exit.store(true, Ordering::SeqCst);
                app_handle.exit(0);
            }
        }
    });
}

pub(crate) fn sync_before_close(db_path: &std::path::Path, backup_database: bool) {
    match AppService::open_with_db_path(db_path.to_path_buf()) {
        Ok(service) => {
            if let Err(error) = service.refresh_asset_mount_statuses(None) {
                eprintln!("failed to sync AssetIWeave mount observations before close: {error}");
            }
        }
        Err(error) => {
            eprintln!("failed to open AssetIWeave database before close: {error}");
        }
    }

    if backup_database {
        match backup_database_from_settings(db_path) {
            Ok(report) => {
                if !report.errors.is_empty() {
                    let errors = report
                        .errors
                        .iter()
                        .map(|error| format!("{}: {}", error.directory, error.message))
                        .collect::<Vec<_>>()
                        .join("; ");
                    eprintln!("AssetIWeave database backup completed with warnings: {errors}");
                }
            }
            Err(error) => {
                eprintln!("failed to back up AssetIWeave database before close: {error}");
            }
        }
    }
}

pub fn run_engine_stdio() {
    if let Err(error) = backend::builtin_skills::install_builtin_skills() {
        eprintln!("failed to install AssetIWeave system Skills: {error}");
        std::process::exit(1);
    }
    if let Err(error) = adapters::engine::run_stdio() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
