# 单卡队列与依赖图

状态由 Issue #24 最新交接决定。本表只定义执行顺序。

| 顺序 | Ticket | 唯一结果 | 前置 |
|---:|---|---|---|
| 1 | [B2-00](tickets/B2-00-baseline.md) | 冻结真实基线并把所有残余命中归卡 | 无 |
| 2 | [B2-R01](tickets/B2-R01-runtime-guards.md) | 建立 AppRuntime 高层 seam 与 runtime 零增长守卫 | B2-00 |
| 3 | [B2-S01](tickets/B2-S01-typed-settings.md) | Settings 原始文档与后端 typed slices 双层契约 | B2-R01 |
| 4 | [B2-R02](tickets/B2-R02-async-dispatcher.md) | Tokio dispatcher 接管调度并删除 thread/Condvar | B2-R01 |
| 5 | [B2-R03](tickets/B2-R03-engine-async-dispatch.md) | Engine registry/transport 建立 async dispatch 边界 | B2-R02 |
| 6 | [B2-R04](tickets/B2-R04-system-tenant-async.md) | Settings/System/Tenant AppService 链路 async-first | B2-S01、B2-R03 |
| 7 | [B2-R05](tickets/B2-R05-catalog-async.md) | Asset/Source/Profile/Catalog 链路 async-first | B2-R04 |
| 8 | [B2-R06](tickets/B2-R06-mount-skill-async.md) | Mount/Group/Skill/Backup 链路 async-first | B2-R05 |
| 9 | [B2-R07](tickets/B2-R07-conversation-adapter-async.md) | Conversation Adapter/Catalog/Sync 链路 async-first | B2-R06 |
| 10 | [B2-R08](tickets/B2-R08-conversation-record-async.md) | Conversation Record/Search/Maintenance 链路 async-first | B2-R07 |
| 11 | [B2-R09](tickets/B2-R09-memory-core-async.md) | Session/Project/Global Memory 链路 async-first | B2-R08 |
| 12 | [B2-R10](tickets/B2-R10-memory-recall-async.md) | Memory Recall/Search/Public 链路 async-first | B2-R09 |
| 13 | [B2-R11](tickets/B2-R11-team-async.md) | Team workflow 与 repository 链路 async-first | B2-R10 |
| 14 | [B2-R12](tickets/B2-R12-agent-market-async.md) | Agent/Market/AI/HTTP 链路 async-first | B2-R11 |
| 15 | [B2-R14](tickets/B2-R14-pool-only-runtime.md) | async bootstrap 与 pool-only Database 成为唯一生产结构 | B2-R12 |
| 16 | [B2-R15](tickets/B2-R15-shutdown-deadline.md) | 单一 deadline 覆盖任务、dispatcher、coordinator 与 pool | B2-R14 |
| 17 | [B2-P01](tickets/B2-P01-process-fixture.md) | process-wrap/which 通过本地跨平台契约验证 | B2-R15 |
| 18 | [B2-P02](tickets/B2-P02-process-migration.md) | 生产 HostProcess 切换并删除手工进程树 | B2-P01 |
| 19 | [B2-L01](tickets/B2-L01-tracing-convergence.md) | tracing span/rolling 接管生产日志 consumer | B2-P02 |
| 20 | [B2-F01](tickets/B2-F01-path-convergence.md) | 应用目录与 UTF-8 边界收口，portable anchors 保真 | B2-L01 |
| 21 | [B2-D01](tickets/B2-D01-sqlx-rows.md) | 高频 SQLx 行映射 typed 化并保留查询语义 | B2-F01 |
| 22 | [B2-Q01](tickets/B2-Q01-errors-warnings-deps.md) | 取消错误合并、warning 收口与依赖审计 | B2-D01 |
| 23 | [B2-G01](tickets/B2-G01-acceptance.md) | 完整行为、删除、跨 surface 与文档验收 | B2-Q01 |

## 默认串行原因

- R03–R12、R14–R15 连续修改 Engine dispatch、`AppService` 签名、adapter 调用方式和测试构造器，必须由编译器逐域驱动，避免多个模型同时制造两套过渡接口。
- P02 与 R12 共享 Agent/Extension 进程 consumer；先完成 runtime 迁移，再替换 OS 机制。
- L01 需要最终 task/tenant runtime context 才能确定 span 边界。
- D01 在 async SQL 调用链稳定后迁移 row mapping，避免同一查询重复改写。
- Q01/G01 负责全局删除证据，不能与前置施工并行。

## 卡片大小规则

每张卡的提交只改变一个 Authority。若 Preflight 发现某领域超过 12 个生产模块或无法在一次目标测试中闭环，先在 Issue #24 把该卡拆成按公开 workflow 命名的连续子卡；拆分不改变 Contract，也不允许同时执行。

## 2026-09-06 审计后队列

旧队列保留历史顺序。当前唯一执行队列和依赖关系见
[`08-remediation-router.md`](08-remediation-router.md)。
