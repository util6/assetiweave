use std::{
    path::PathBuf,
    sync::{atomic::AtomicBool, Arc, Mutex},
};

pub(crate) struct AppState {
    pub(crate) db_path: PathBuf,
    pub(crate) lock: Arc<Mutex<()>>,
    pub(crate) background_tasks:
        Arc<crate::adapters::tauri::background_tasks::BackgroundTaskRegistry>,
    pub(crate) allow_close: Arc<AtomicBool>,
    pub(crate) allow_exit: Arc<AtomicBool>,
    pub(crate) exit_prompt_open: Arc<AtomicBool>,
}
