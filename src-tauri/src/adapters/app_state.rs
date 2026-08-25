use std::{
    path::PathBuf,
    sync::{atomic::AtomicBool, Arc},
};

/// 桌面应用全局共享状态
///
/// 在 Tauri 应用中托管全局互斥锁、数据库路径、后台任务注册表以及应用退出保护标志。
pub(crate) struct AppState {
    /// SQLite 数据库文件的本地绝对路径
    pub(crate) db_path: PathBuf,
    /// 进程级共享资源宿主；请求从该实例绑定 AppService。
    pub(crate) runtime: Arc<crate::backend::runtime::AppRuntime>,
    /// 后台长运行任务（如扫描、备份、目录挂载等）的中央注册表
    pub(crate) background_tasks:
        Arc<crate::adapters::tauri::background_tasks::BackgroundTaskRegistry>,
    /// 跨命令共享的 Agent 执行 Runtime；并发限制与 Agent Registry 由它统一持有
    pub(crate) agent_runtime: Arc<dyn crate::backend::ai_execution::AgentExecutionRuntime>,
    /// 是否允许关闭主窗口标记
    pub(crate) allow_close: Arc<AtomicBool>,
    /// 是否允许应用完全退出标记
    pub(crate) allow_exit: Arc<AtomicBool>,
    /// 当前是否正在向用户展示退出确认弹窗标记
    pub(crate) exit_prompt_open: Arc<AtomicBool>,
    /// 关机/退出前的同步清理工作是否已完成标记
    pub(crate) shutdown_sync_done: Arc<AtomicBool>,
}
