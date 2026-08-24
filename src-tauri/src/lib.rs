mod adapters;
mod backend;

use crate::{
    adapters::{app_state::AppState, tauri::background_tasks::BackgroundTaskRegistry},
    backend::{
        application::AppService,
        data_backup::backup_database_from_settings_value,
        logs::write_startup_log,
        operation_log::{log_error, log_warn},
        path_utils::app_db_path,
        runtime::{AppRuntime, RuntimeRole},
    },
};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use tauri::{Emitter, Manager};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};

const APP_CLOSE_REQUESTED_EVENT: &str = "app-close-requested";
const STARTUP_SELF_CHECK_ARG: &str = "--startup-self-check";
const AI_EXECUTION_CLOSE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);
const AI_EXECUTION_CLOSE_POLL_INTERVAL: std::time::Duration = std::time::Duration::from_millis(25);

pub(crate) async fn converge_ai_executions_before_close(
    background_tasks: Arc<BackgroundTaskRegistry>,
) {
    match background_tasks
        .cancel_ai_executions_and_wait(AI_EXECUTION_CLOSE_TIMEOUT, AI_EXECUTION_CLOSE_POLL_INTERVAL)
        .await
    {
        Ok(report) if !report.converged => log_warn(
            "app.close.ai_execution",
            "HIGH PRIORITY: AI execution cleanup did not converge before app close",
            &[
                ("cancelled_count", report.cancelled_count.to_string()),
                ("remaining_count", report.remaining_count.to_string()),
            ],
        ),
        Ok(_) => {}
        Err(error) => log_error(
            "app.close.ai_execution",
            "HIGH PRIORITY: failed to cancel AI executions before app close",
            &error,
            &[],
        ),
    }
}

fn setup_panic_hook() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let payload = if let Some(s) = info.payload().downcast_ref::<&str>() {
            (*s).to_string()
        } else if let Some(s) = info.payload().downcast_ref::<String>() {
            s.clone()
        } else {
            "Unknown panic payload".to_string()
        };
        let location = info
            .location()
            .map(|loc| format!("{}:{}:{}", loc.file(), loc.line(), loc.column()))
            .unwrap_or_else(|| "unknown location".to_string());
        let backtrace = std::backtrace::Backtrace::force_capture();
        let panic_message =
            format!("Panic occurred at {location}: {payload}\nBacktrace:\n{backtrace}");
        eprintln!("{panic_message}");
        crate::backend::logs::record_fatal_panic(&panic_message);
        default_hook(info);
    }));
}

fn has_startup_self_check_arg<I, S>(args: I) -> bool
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    args.into_iter()
        .any(|argument| argument.as_ref() == STARTUP_SELF_CHECK_ARG)
}

fn run_startup_self_check(_context: tauri::Context<tauri::Wry>) -> Result<(), String> {
    backend::builtin_skills::install_builtin_skills()
        .map_err(|error| format!("内置 Skill 校验或安装失败: {error}"))?;
    let db_path = app_db_path().map_err(|error| format!("数据库路径初始化失败: {error}"))?;
    let runtime = AppRuntime::bootstrap(db_path, RuntimeRole::OneShot)
        .map_err(|error| format!("数据库和运行时初始化失败: {error}"))?;
    let report = runtime.shutdown_with_grace(std::time::Duration::from_secs(5));
    if !report.unfinished_task_ids.is_empty()
        || !report.dispatcher_drained
        || report.dispatcher_remaining_events > 0
        || report.dispatcher_timed_out
    {
        return Err(format!(
            "运行时关闭自检失败: unfinished_tasks={}, dispatcher_drained={}, remaining_events={}, timed_out={}",
            report.unfinished_task_ids.len(),
            report.dispatcher_drained,
            report.dispatcher_remaining_events,
            report.dispatcher_timed_out
        ));
    }
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    setup_panic_hook();
    let context: tauri::Context<tauri::Wry> = tauri::generate_context!();
    if has_startup_self_check_arg(std::env::args()) {
        match run_startup_self_check(context) {
            Ok(()) => return,
            Err(error) => {
                let message = format!("AssetIWeave startup self-check failed: {error}");
                log_error("app.startup.self_check", "启动自检失败", &error, &[]);
                crate::backend::logs::record_fatal_panic(&message);
                eprintln!("{message}");
                std::process::exit(1);
            }
        }
    }

    if let Err(error) = backend::builtin_skills::install_builtin_skills() {
        log_error(
            "app.startup.skills",
            "failed to install AssetIWeave system Skills",
            &error,
            &[],
        );
        panic!("failed to install AssetIWeave system Skills: {error}");
    }
    let db_path = match app_db_path() {
        Ok(path) => path,
        Err(error) => {
            log_error(
                "app.startup.db_path",
                "failed to resolve AssetIWeave database path",
                &error,
                &[],
            );
            panic!("failed to resolve AssetIWeave database path: {error}");
        }
    };
    let runtime = match AppRuntime::bootstrap(db_path.clone(), RuntimeRole::ResidentHost) {
        Ok(runtime) => runtime,
        Err(error) => {
            log_error(
                "app.startup.runtime",
                "failed to initialize AssetIWeave AppRuntime",
                &error,
                &[],
            );
            panic!("failed to initialize AssetIWeave AppRuntime: {error}");
        }
    };
    if let Err(error) = backend::runtime::install_process_runtime(runtime.clone()) {
        log_error(
            "app.startup.runtime_install",
            "failed to install the resident AppRuntime as the process settings authority",
            &error,
            &[],
        );
        panic!("failed to install AssetIWeave process AppRuntime: {error}");
    }
    let agent_runtime = runtime.agent_runtime();
    let conversation_full_sync_on_startup_enabled =
        match backend::app_settings::conversation_full_sync_on_startup_enabled_for_database(
            runtime.db(),
        ) {
            Ok(enabled) => enabled,
            Err(error) => {
                log_error(
                    "app.startup.conversation_sync_setting",
                    "failed to read Conversation startup sync setting",
                    &error,
                    &[],
                );
                true
            }
        };
    let conversation_payload_policy_reparse_required = {
        let service = AppService::from_runtime(&runtime);
        if let Err(error) = service.interrupt_stale_memory_runs() {
            log_error(
                "app.startup.memory_recovery",
                "failed to mark interrupted Memory runs on startup",
                &error,
                &[],
            );
        }
        if let Err(error) = service.refresh_recorded_assets() {
            log_error(
                "app.startup.asset_refresh",
                "failed to validate recorded AssetIWeave assets on startup",
                &error,
                &[],
            );
        }
        if let Err(error) = service.refresh_asset_mount_statuses(None) {
            log_error(
                "app.startup.mount_refresh",
                "failed to sync AssetIWeave mount observations on startup",
                &error,
                &[],
            );
        }
        match service.conversation_payload_policy_reparse_required() {
            Ok(required) => required,
            Err(error) => {
                log_error(
                    "app.startup.conversation_policy",
                    "failed to inspect Conversation payload policy state",
                    &error,
                    &[],
                );
                false
            }
        }
    };
    if let Err(error) = write_startup_log() {
        log_error(
            "app.startup.log",
            "failed to write AssetIWeave startup log",
            &error,
            &[],
        );
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
                    let background_tasks = state.background_tasks.clone();
                    let runtime = state.runtime.clone();
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
                                tauri::async_runtime::spawn(async move {
                                    converge_ai_executions_before_close(background_tasks).await;
                                    let _ = tauri::async_runtime::spawn_blocking(move || {
                                        runtime.shutdown_with_grace(
                                            std::time::Duration::from_secs(5),
                                        )
                                    })
                                    .await;
                                    allow_close.store(true, Ordering::SeqCst);
                                    allow_exit.store(true, Ordering::SeqCst);
                                    if let Err(error) = close_window.close() {
                                        log_error(
                                            "app.close.window",
                                            "failed to close AssetIWeave after confirmation",
                                            &error,
                                            &[],
                                        );
                                    }
                                });
                            }
                        });
                    return;
                }

                api.prevent_close();
                if state.exit_prompt_open.swap(true, Ordering::SeqCst) {
                    return;
                }
                if let Err(error) = window.emit(APP_CLOSE_REQUESTED_EVENT, ()) {
                    log_error(
                        "app.close.request",
                        "failed to notify frontend about close request",
                        &error,
                        &[],
                    );
                    state.exit_prompt_open.store(false, Ordering::SeqCst);
                    state.allow_close.store(true, Ordering::SeqCst);
                    state.allow_exit.store(true, Ordering::SeqCst);
                    if let Err(close_error) = window.close() {
                        log_error(
                            "app.close.window",
                            "failed to close AssetIWeave after close prompt notification error",
                            &close_error,
                            &[],
                        );
                    }
                }
            }
        })
        .manage(AppState {
            db_path,
            runtime: runtime.clone(),
            background_tasks: Arc::new(BackgroundTaskRegistry::with_task_runtime(
                runtime.task_runtime().clone(),
            )),
            agent_runtime,
            allow_close: Arc::new(AtomicBool::new(false)),
            allow_exit: Arc::new(AtomicBool::new(false)),
            exit_prompt_open: Arc::new(AtomicBool::new(false)),
            shutdown_sync_done: Arc::new(AtomicBool::new(false)),
        })
        .invoke_handler(adapters::tauri::command_handler())
        .build(context)
        .unwrap_or_else(|error| {
            log_error("app.startup.tauri", "error while running AssetIWeave", &error, &[]);
            panic!("error while running AssetIWeave: {error}");
        });
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
            state.runtime.clone(),
            state.background_tasks.clone(),
            params,
        ) {
            log_error(
                "app.startup.conversation_policy_reparse",
                "failed to start Conversation payload policy reparse",
                &error,
                &[],
            );
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
                let background_tasks = state.background_tasks.clone();
                let runtime = state.runtime.clone();
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
                            tauri::async_runtime::spawn(async move {
                                converge_ai_executions_before_close(background_tasks).await;
                                let _ = tauri::async_runtime::spawn_blocking(move || {
                                    runtime.shutdown_with_grace(
                                        std::time::Duration::from_secs(5),
                                    )
                                })
                                .await;
                                allow_exit.store(true, Ordering::SeqCst);
                                exit_app.exit(0);
                            });
                        }
                    });
                return;
            }

            api.prevent_exit();
            if state.exit_prompt_open.swap(true, Ordering::SeqCst) {
                return;
            }
            if let Err(error) = app_handle.emit(APP_CLOSE_REQUESTED_EVENT, ()) {
                log_error(
                    "app.exit.request",
                    "failed to notify frontend about app exit request",
                    &error,
                    &[],
                );
                state.exit_prompt_open.store(false, Ordering::SeqCst);
                state.allow_exit.store(true, Ordering::SeqCst);
                app_handle.exit(0);
            }
        }
    });
}

pub(crate) fn sync_before_close_with_runtime(
    runtime: &Arc<AppRuntime>,
    db_path: &std::path::Path,
    backup_database: bool,
) {
    let service = AppService::from_runtime(runtime);
    if let Err(error) = service.refresh_asset_mount_statuses(None) {
        log_error(
            "app.close.mount_refresh",
            "failed to sync AssetIWeave mount observations before close",
            &error,
            &[],
        );
    }

    if backup_database {
        let settings = match service.get_app_settings() {
            Ok(settings) => settings.settings,
            Err(error) => {
                log_error(
                    "app.close.database_backup_settings",
                    "读取 SQLite 备份设置失败，使用默认备份目录",
                    &error,
                    &[],
                );
                serde_json::Value::Object(Default::default())
            }
        };
        let backup_result = backup_database_from_settings_value(db_path, &settings);
        match backup_result {
            Ok(report) => {
                if !report.errors.is_empty() {
                    let errors = report
                        .errors
                        .iter()
                        .map(|error| format!("{}: {}", error.directory, error.message))
                        .collect::<Vec<_>>()
                        .join("; ");
                    log_warn(
                        "app.close.database_backup",
                        "AssetIWeave database backup completed with warnings",
                        &[("errors", errors)],
                    );
                }
            }
            Err(error) => {
                log_error(
                    "app.close.database_backup",
                    "failed to back up AssetIWeave database before close",
                    &error,
                    &[],
                );
            }
        }
    }
}

pub fn run_engine_stdio() {
    if let Err(error) = backend::builtin_skills::install_builtin_skills() {
        eprintln!("failed to install AssetIWeave system Skills: {error}");
        std::process::exit(1);
    }
    let engine_db_path = std::env::var("ASSETIWEAVE_DB_PATH")
        .ok()
        .filter(|path| !path.trim().is_empty())
        .map(std::path::PathBuf::from)
        .map(Ok)
        .unwrap_or_else(backend::path_utils::app_db_path);
    let runtime = match engine_db_path {
        Ok(path) => match AppRuntime::bootstrap(path, RuntimeRole::OneShot) {
            Ok(runtime) => {
                if let Err(error) = backend::runtime::install_process_runtime(runtime.clone()) {
                    eprintln!("failed to install Engine AppRuntime: {error}");
                    std::process::exit(1);
                }
                runtime.agent_runtime()
            }
            Err(error) => {
                eprintln!("failed to initialize Engine AppRuntime: {error}");
                std::process::exit(1);
            }
        },
        Err(error) => {
            eprintln!("failed to initialize Engine agent runtime: {error}");
            std::process::exit(1);
        }
    };
    if let Err(error) = install_engine_termination_handlers(runtime) {
        eprintln!("failed to install Engine termination handlers: {error}");
        std::process::exit(1);
    }
    if let Err(error) = adapters::engine::run_stdio() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

fn install_engine_termination_handlers(
    runtime: Arc<dyn backend::ai_execution::AgentExecutionRuntime>,
) -> Result<(), String> {
    #[cfg(unix)]
    {
        let termination_requested = Arc::new(std::sync::atomic::AtomicBool::new(false));
        for signal in [signal_hook::consts::SIGINT, signal_hook::consts::SIGTERM] {
            signal_hook::flag::register(signal, termination_requested.clone())
                .map_err(|error| error.to_string())?;
        }
        std::thread::spawn(move || {
            while !termination_requested.load(Ordering::Acquire) {
                std::thread::sleep(std::time::Duration::from_millis(20));
            }
            runtime.cancel_all();
        });
    }
    Ok(())
}

pub(crate) fn sync_before_close(db_path: &std::path::Path, backup_database: bool) {
    match AppRuntime::bootstrap(db_path.to_path_buf(), RuntimeRole::OneShot) {
        Ok(runtime) => sync_before_close_with_runtime(&runtime, db_path, backup_database),
        Err(error) => log_error(
            "app.close.database",
            "failed to open AssetIWeave database before close",
            &error,
            &[],
        ),
    }
}


#[cfg(test)]
mod tests {
    use super::has_startup_self_check_arg;

    #[test]
    fn recognizes_startup_self_check_argument_without_matching_similar_values() {
        assert!(has_startup_self_check_arg([
            "assetiweave",
            "--startup-self-check"
        ]));
        assert!(!has_startup_self_check_arg([
            "assetiweave",
            "--startup-self-check=true"
        ]));
    }
}
