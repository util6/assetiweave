mod adapters;
mod backend;

use crate::{
    adapters::{app_state::AppState, tauri::background_tasks::BackgroundTaskRegistry},
    backend::{application::AppService, logs::write_startup_log, path_utils::app_db_path},
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
        let service = AppService::open_with_db_path(db_path.clone())
            .expect("failed to initialize AssetIWeave database");
        if let Err(error) = service.refresh_recorded_assets() {
            eprintln!("failed to validate recorded AssetIWeave assets on startup: {error}");
        }
        if let Err(error) = service.refresh_asset_mount_statuses(None) {
            eprintln!("failed to sync AssetIWeave mount observations on startup: {error}");
        }
    }
    if let Err(error) = write_startup_log() {
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
            background_tasks: Arc::new(BackgroundTaskRegistry::default()),
            allow_close: Arc::new(AtomicBool::new(false)),
            allow_exit: Arc::new(AtomicBool::new(false)),
            exit_prompt_open: Arc::new(AtomicBool::new(false)),
        })
        .invoke_handler(adapters::tauri::command_handler())
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
}

pub fn run_engine_stdio() {
    if let Err(error) = adapters::engine::run_stdio() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
