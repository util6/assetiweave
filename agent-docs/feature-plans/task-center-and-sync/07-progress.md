# 进度追踪：Task Center & Adapter Concurrency (Issue #33)

## 状态看板

- 当前阶段：实施完成，全仓门禁通过
- 开始基线：`12ec758a`

| 工单 | 状态 | 负责人 | 验证结果 | 提交/证据 |
| :--- | :--- | :--- | :--- | :--- |
| T01 | Completed | Agent | Pass | TaskRuntime 内存改造、容量 50 上限、clear_terminal、活动模型 |
| T02 | Completed | Agent | Pass | AppService 统一收口与 Task View 契约 (tasks_public.rs) |
| T03 | Completed | Agent | Pass | Tauri Commands 映射与契约同构 |
| T04 | Completed | Agent | Pass | Engine stdio JSON 接口与契约生成 (pnpm cli:contract) |
| T05 | Completed | Agent | Pass | HostProcess 异步按行流式读取 stdout/stderr |
| T06 | Completed | Agent | Pass | NDJSON Progress 协议与安全脱敏清洗 |
| T07 | Completed | Agent | Pass | Conversation Adapter 分组并发门控 (默认 4 组，同组串行) |
| T08 | Completed | Agent | Pass | 8 个内置 Adapter 升级打包并更新 content_hash |
| T09 | Completed | Agent | Pass | Session Memory 5 阶段状态机流转与审计映射 |
| T10 | Completed | Agent | Pass | TaskCenterProvider、前端 Service 与单元测试 |
| T11 | Completed | Agent | Pass | 全局设置 showTaskNotifications 开关与多语言支持 |
| T12 | Completed | Agent | Pass | 侧边栏通知气泡 TaskNotificationBubble 及单测 |
| T13 | Completed | Agent | Pass | 移除散落悬浮状态卡，收归侧边栏气泡与统一通知 |
| T14 | Completed | Agent | Pass | 侧边栏任务图标入口 (ListTodo)、运行中徽标与路由注册 |
| T15 | Completed | Agent | Pass | 任务中心页面 (主从布局、Stage 流水线、Worker 活动、指标与审计) |
| T16 | Completed | Agent | Pass | 全仓门禁（typecheck, pnpm test, cargo test, go vet/test, e2e）全数通过 |

