# 契约定义：公开 Task View、Adapter Progress 与通知设置 (Issue #33)

## 1. 统一公开 Task View 契约 (Rust / DTO)

```rust
pub struct TaskView {
    pub id: String,
    pub kind: String, // TaskKind 序列化字符串
    pub title: String,
    pub tenant_id: Option<String>,
    pub state: String, // pending, running, cancelling, succeeded, failed, canceled
    pub outcome: Option<String>, // success, partial_success, failure, canceled
    pub started_at: String,
    pub updated_at: String,
    pub finished_at: Option<String>,
    pub progress: Option<TaskProgress>,
    pub stages: Vec<TaskStageView>,
    pub metrics: Vec<TaskMetricView>,
    pub failures: Vec<TaskFailureView>,
    pub error_summary: Option<String>,
    pub result_summary: Option<String>,
    pub capabilities: TaskCapabilitiesView,
    pub revision: u64,
}

pub struct TaskStageView {
    pub id: String,
    pub name: String,
    pub status: String, // pending, running, succeeded, partial_success, failed, canceled, skipped
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub duration_ms: Option<u64>,
    pub progress: Option<TaskProgress>,
    pub current_activities: Vec<TaskActivityView>,
    pub metrics: Vec<TaskMetricView>,
    pub failures: Vec<TaskFailureView>,
    pub skipped: Vec<TaskSkippedReasonView>,
}

pub struct TaskActivityView {
    pub worker_id: String,
    pub operation: String,
    pub path: Option<String>,
    pub display_path: Option<String>,
    pub started_at: String,
    pub current: Option<u64>,
    pub total: Option<u64>,
}

pub struct TaskFailureView {
    pub code: String,
    pub message: String,
    pub stage: String,
    pub identity: Option<String>,
    pub retryable: bool,
    pub path: Option<String>,
    pub timestamp: String,
}

pub struct TaskSkippedReasonView {
    pub reason_code: String,
    pub count: u64,
    pub samples: Vec<String>,
}

pub struct TaskMetricView {
    pub code: String,
    pub value: u64,
}

pub struct TaskCapabilitiesView {
    pub cancellable: bool,
    pub retryable: bool,
    pub clearable: bool,
}
```

## 2. Conversation Adapter NDJSON 协议扩展

向后兼容扩展，旧 Adapter 不发 progress 或缺少字段时按 Source 级兼容降级。

```json
{"type": "progress", "stage": "reading", "operation": "read_file", "path": "relative/or/full/path", "current": 3, "total": 10, "worker": "worker-1"}
```

Core 规则：
- 校验并规范化路径，脱敏 home 目录；
- 原位覆盖对应 Worker 之前的旧 Activity；
- 成功处理完后该 Activity 移除，进入 metrics 计数；
- Stage 结束时清空该 Stage 下所有 Activity。

## 3. 设置契约

- 导航面板新增：`"general.notifications"`
- 设置项：
  - `showStartupNotification: boolean`（从外观面板迁移至此）
  - `showTaskNotifications: boolean`（默认 `true`）
- 保持后端 `BackendSettings` 透传与序列化支持，保存时不丢失。
