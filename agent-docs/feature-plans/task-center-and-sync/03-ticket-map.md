# 工单分解图：Task Center & Adapter Concurrency (Issue #33)

| 工单编号 | 目标阶段 | 描述 | 关键产出/断言 |
| :--- | :--- | :--- | :--- |
| **T01** | Phase 1 | TaskRuntime 数据结构与生命周期扩展 | `TaskStage`, `TaskActivity`, `TaskOutcome`, 50 上限最旧终态淘汰, `clear_terminal` |
| **T02** | Phase 1 | AppService 统一 Task View 契约与脱敏投影 | `TaskView` DTO、脱敏规则、AppService public task 方法 |
| **T03** | Phase 1 | 关键节点事件与节流推送 | `task-updated` 广播、普通 activity 10次/秒节流、终态即时推送 |
| **T04** | Phase 1 | Tauri 命令与 CLI 契约同构 | Tauri commands、`pnpm cli:contract` |
| **T05** | Phase 2 | HostProcess 流式 stdout 与 Adapter progress 解析 | `run_host_command_streaming`、NDJSON `progress` 解析、向后兼容 |
| **T06** | Phase 2 | 内置 Adapter 升级输出 progress | Codex, Claude Code, OpenCode, AntiGravity, Zcode progress 输出与测试 |
| **T07** | Phase 2 | Conversation 同步 Adapter 分组并发调度 | 按 Adapter 分组、4 组并发门、组内 Source 串行、错误隔离、partial_success |
| **T08** | Phase 3 | Memory 专用 Task View 阶段映射 | Session/Project/Global 阶段（领取、加载、Agent、校验、发布）、安全摘要 |
| **T09** | Phase 3 | 通用后台任务接入公开 Task View | AI 执行、扫描、索引、备份、批量挂载、Team Run 通用状态与操作 |
| **T10** | Phase 4 | 前端 Task Center Service & Provider | `taskCenterService`, `TaskCenterProvider` 统一事件+轮询、单调 revision 合并 |
| **T11** | Phase 4 | 设置“通用 → 通知”面板与配置扩充 | `general.notifications`, `showTaskNotifications` (默认 true), 后端透传防丢 |
| **T12** | Phase 4 | 侧边栏“任务、日志、设置”与路由收口 | `/tasks` 路由、不可隐藏任务入口、活动徽标与失败报警标记 |
| **T13** | Phase 5 | Task Center 流程优先主从工作区页面 | 左侧列表+右侧详情（指标、时间线、Worker 活动、审计展开、操作） |
| **T14** | Phase 5 | 侧边栏锚定单条气泡通知组件 | 4s/8s 停留、悬停暂停、3 条有界队列、点击跳转、静默控制 |
| **T15** | Phase 5 | 清理旧业务悬浮指示卡组件 | 移除各业务悬浮任务条与根部挂载 |
| **T16** | Phase 6 | 全仓测试、并发验证与门禁合规 | 全自动化测试通过、`cargo fmt`, `pnpm typecheck`, `pnpm test`, `go test` |
| **T17** | Phase 7 | ADR-0017 决策落地、SQLite Task Journal 与冷启动自愈 | ADR-0017、`task_journal` 表迁移、优雅停机序列、`reconcile_orphaned_tasks_on_startup` |
| **T18** | Phase 7 | AppService 流水线脚手架、双轨事件信封与保序节流 | `spawn_pipeline_task`、`run_inline`、`ConflictPolicy`、`stages::*` 常量、`TaskChangeClass`、150ms 节流 + 冲刷保序 |
| **T19** | Phase 7 | `scan_sources` 黄金切片垂直改穿、旧轨物理删除与全仓门禁 | 迁移 `scan_sources` 为三阶段流水线、物理删除 `SourceScanTaskSnapshot` 与私有事件、前端 `taskInvalidationRegistry`、契约与全仓测试全绿 |

