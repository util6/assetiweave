use super::task_pipeline::StageGuard;
use super::tasks::{TaskRuntime, TaskSpec};
use super::{AppError, AppResult, TaskHandle, TaskOutput};
use process_wrap::tokio::CommandWrap;
#[cfg(windows)]
use process_wrap::tokio::JobObject;
#[cfg(unix)]
use process_wrap::tokio::ProcessGroup;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::mpsc;

/// 外部进程调用规格
#[derive(Debug, Clone)]
pub struct ProcessCommandSpec {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub env: Vec<(String, String)>,
    pub current_dir: Option<PathBuf>,
    pub stage_id: String,
    pub kill_timeout: Duration,
}

impl ProcessCommandSpec {
    pub fn new(program: impl Into<PathBuf>, stage_id: impl Into<String>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
            env: Vec::new(),
            current_dir: None,
            stage_id: stage_id.into(),
            kill_timeout: Duration::from_secs(3),
        }
    }

    pub fn arg(mut self, arg: impl Into<String>) -> Self {
        self.args.push(arg.into());
        self
    }

    pub fn args(mut self, args: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.args.extend(args.into_iter().map(Into::into));
        self
    }

    pub fn env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.push((key.into(), value.into()));
        self
    }

    pub fn current_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.current_dir = Some(dir.into());
        self
    }

    pub fn kill_timeout(mut self, timeout: Duration) -> Self {
        self.kill_timeout = timeout;
        self
    }
}

/// NDJSON 流式行协议约定
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ProcessProtocolMessage {
    Progress {
        current: u64,
        total: Option<u64>,
        note: Option<String>,
    },
    Activity {
        worker_id: String,
        operation: String,
        display_path: Option<String>,
        current: Option<u64>,
        total: Option<u64>,
    },
    RemoveActivity {
        worker_id: String,
    },
    Metric {
        code: String,
        value: u64,
    },
    Skipped {
        reason_code: String,
        sample: String,
    },
    Failure {
        code: String,
        message: String,
        retryable: bool,
    },
    ResultSummary {
        summary: String,
    },
}

/// 进程执行终态结果
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcessExecutionResult {
    pub exit_code: Option<i32>,
    pub summary: Option<String>,
}

/// 通用子进程流式执行器
pub struct ProcessRunner;

impl ProcessRunner {
    /// 在指定的流水线阶段中启动并流式消费外部命令
    pub async fn run_process_in_stage(
        guard: &mut StageGuard,
        spec: ProcessCommandSpec,
    ) -> AppResult<ProcessExecutionResult> {
        let mut cmd = tokio::process::Command::new(&spec.program);
        cmd.args(&spec.args);
        for (k, v) in &spec.env {
            cmd.env(k, v);
        }
        if let Some(dir) = &spec.current_dir {
            cmd.current_dir(dir);
        }
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        #[cfg(unix)]
        let mut wrap = CommandWrap::from(cmd);
        #[cfg(unix)]
        let wrap = wrap.wrap(ProcessGroup::leader());

        #[cfg(windows)]
        let mut wrap = CommandWrap::from(cmd);
        #[cfg(windows)]
        let wrap = wrap.wrap(JobObject);

        let mut child = wrap.spawn().map_err(|e| {
            AppError::external(format!("启动子进程失败 ({}): {e}", spec.program.display()))
        })?;

        let stdout = child
            .stdout()
            .take()
            .ok_or_else(|| AppError::external("未能获取子进程 stdout 管道".to_string()))?;
        let stderr = child
            .stderr()
            .take()
            .ok_or_else(|| AppError::external("未能获取子进程 stderr 管道".to_string()))?;

        let (proto_tx, mut proto_rx) = mpsc::channel::<ProcessProtocolMessage>(128);
        let stage_id_out = spec.stage_id.clone();
        let stage_id_err = spec.stage_id.clone();

        // 异步流读取 stdout，解析 NDJSON 或回退为日志
        let stdout_task = tokio::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let line_text: &str = line.as_str();
                if let Ok(msg) = serde_json::from_str::<ProcessProtocolMessage>(line_text) {
                    if proto_tx.send(msg).await.is_err() {
                        break;
                    }
                } else {
                    tracing::info!(stage = %stage_id_out, message = %line);
                }
            }
        });

        // 异步流读取 stderr，记录为警告日志
        let stderr_task = tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                tracing::warn!(stage = %stage_id_err, message = %line);
            }
        });

        let mut summary = None;
        let cancellation = guard.cancellation().clone();

        // 主驱动循环：监听协议消息、进程退出与取消信号
        let exit_status = loop {
            tokio::select! {
                _ = cancellation.cancelled() => {
                    tracing::info!(stage = %spec.stage_id, "接收到取消信号，向子进程组发送强杀请求");
                    let _ = child.start_kill();
                    let _ = stdout_task.await;
                    let _ = stderr_task.await;
                    return Err(AppError::Cancelled("后台任务已取消".to_string()));
                }
                Some(msg) = proto_rx.recv() => {
                    match msg {
                        ProcessProtocolMessage::Progress { current, total, note } => {
                            guard.progress(current, total, note);
                        }
                        ProcessProtocolMessage::Activity { worker_id, operation, display_path, current, total } => {
                            let tracker = guard.worker(&worker_id);
                            tracker.report(operation, current, total, display_path);
                        }
                        ProcessProtocolMessage::RemoveActivity { worker_id } => {
                            guard.remove_activity(&worker_id);
                        }
                        ProcessProtocolMessage::Metric { code, value } => {
                            guard.record_metric(code, value);
                        }
                        ProcessProtocolMessage::Skipped { reason_code, sample } => {
                            guard.record_skipped(reason_code, sample);
                        }
                        ProcessProtocolMessage::Failure { code, message, retryable } => {
                            guard.record_failure(code, message, retryable);
                        }
                        ProcessProtocolMessage::ResultSummary { summary: s } => {
                            summary = Some(s);
                        }
                    }
                }
                status_res = child.wait() => {
                    // 等待日志管道完全消费排空
                    let _ = stdout_task.await;
                    let _ = stderr_task.await;

                    // 排空管道中可能滞留的协议行
                    while let Ok(msg) = proto_rx.try_recv() {
                        match msg {
                            ProcessProtocolMessage::Progress { current, total, note } => {
                                guard.progress(current, total, note);
                            }
                            ProcessProtocolMessage::Activity { worker_id, operation, display_path, current, total } => {
                                let tracker = guard.worker(&worker_id);
                                tracker.report(operation, current, total, display_path);
                            }
                            ProcessProtocolMessage::RemoveActivity { worker_id } => {
                                guard.remove_activity(&worker_id);
                            }
                            ProcessProtocolMessage::Metric { code, value } => {
                                guard.record_metric(code, value);
                            }
                            ProcessProtocolMessage::Skipped { reason_code, sample } => {
                                guard.record_skipped(reason_code, sample);
                            }
                            ProcessProtocolMessage::Failure { code, message, retryable } => {
                                guard.record_failure(code, message, retryable);
                            }
                            ProcessProtocolMessage::ResultSummary { summary: s } => {
                                summary = Some(s);
                            }
                        }
                    }

                    break status_res.map_err(|e| AppError::external(format!("等待子进程退出失败: {e}")))?;
                }
            }
        };

        let exit_code = exit_status.code();
        if !exit_status.success() {
            let err_msg = format!("子进程以非零状态码退出: {:?}", exit_code);
            guard.fail("process_exit_error", &err_msg, false);
            return Err(AppError::external(err_msg));
        }

        Ok(ProcessExecutionResult { exit_code, summary })
    }
}

impl TaskRuntime {
    /// 运行外部子进程流式任务，自动对接任务中台流水线
    pub fn run_process(
        &self,
        spec: TaskSpec,
        process_spec: ProcessCommandSpec,
    ) -> AppResult<TaskHandle<ProcessExecutionResult>> {
        self.run(spec, move |context| async move {
            let mut guard = context.enter_stage(&process_spec.stage_id);
            let result = ProcessRunner::run_process_in_stage(&mut guard, process_spec).await?;
            drop(guard);

            let summary = result
                .summary
                .clone()
                .unwrap_or_else(|| format!("子进程执行完成 (exit code: {:?})", result.exit_code));

            Ok(TaskOutput::with_summary(result, summary))
        })
    }
}

#[cfg(test)]
#[path = "task_process_tests.rs"]
mod tests;
