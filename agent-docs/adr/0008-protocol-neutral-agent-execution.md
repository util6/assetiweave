# 0008: 以协议中立的 Agent 定义路由 AI 执行

> 状态：已接受
> 决策日期：2026-08-13
> 决策证据：`37cefac`
> 记录日期：2026-08-23（本次文档迁移）

业务工作流只依赖 `AgentExecutor`、`AgentDefinition` 与统一的执行请求/结果；由协议字段选择 ACP 或 Native 后端，而不是由 Agent ID、供应商名称或具体 CLI 决定控制流。这样增加 Agent 或协议不会把供应商分支扩散到 Translation、Tauri、Engine 与业务工作流，同时保留协议层和进程层各自的边界。

来源：`agent-docs/feature-plans/acp-agent-execution-runtime/02-architecture-design.md`。
