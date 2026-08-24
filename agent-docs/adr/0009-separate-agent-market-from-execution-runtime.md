# 0009: 分离 Agent 市场、安装状态与执行 Registry

> 状态：已接受
> 决策日期：2026-08-17
> 决策证据：`aa1080f`
> 记录日期：2026-08-23（本次文档迁移）

Agent 市场目录只描述可安装项；SQLite 只保存当前安装与健康摘要；执行路径只读取不可变的 `AgentRegistry` 快照。安装、更新和卸载在后台生命周期工作流中完成物化、验证、持久化与快照切换，执行 Runtime 不读取远程目录或直接管理分发。这把网络和安装失败隔离在生命周期阶段，也让 Desktop、Engine 与 CLI 复用同一 `AppService` 工作流。

来源：`agent-docs/feature-plans/agent-marketplace-dynamic-runtime/02-architecture-design.md`。
