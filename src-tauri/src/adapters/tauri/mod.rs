//! Tauri 前端 Bridge 适配模块
//!
//! 包含前端调用的 Tauri Command 命令入口绑定以及后台异步任务注册与事件推送到前端的机制。

pub(crate) mod app_icon;
pub(crate) mod background_tasks;
pub(crate) mod commands;

pub(crate) use commands::command_handler;
