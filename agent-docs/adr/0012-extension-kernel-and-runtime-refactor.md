# 0012: 参照 DeepSeek Harness 与 Pi Agent 重构 Extension Kernel 微内核与全系统运行时收口

> **重要等级**：最高基石重塑（P0+ - 全生命周期最大规模架构重构）  
> **状态**：已接受（全面替代早期 Legacy 执行栈、旧版独立命令壳与单体全局锁）  
> **参考项目**：[deepseek-ai/deepseek-harness](https://github.com/deepseek-ai/deepseek-harness)（Honeycomb 架构参考）、[earendil-works/pi](https://github.com/earendil-works/pi)（Pi Agent / pi-mono 微内核 Harness 范式）  
> **决策日期**：2026-08-19  
> **决策证据**：`c9b9964`, `5ecfabd`, `0baccb5`, `a49f790`, `01a01061`, `01a012fb`  
> **记录日期**：2026-08-25  

## 背景

随着 AssetIWeave 接入了 Agent 市场和 Conversation Adapter 两大生态，系统出现了显著的架构异构与重复建设：
1. **双市场底层重复**：Agent 扩展包与 Adapter 扩展包各自拥有独立的安装器、版本管理、探活探测和错误结构，底层抽象严重冗余。
2. **全局阻塞锁瓶颈**：后端长期依赖粗粒度的全局应用锁（Global Lock），在执行耗时 I/O（扫描 Source、同步会话、拉取依赖包、大批量挂载）时会直接阻塞整个 AppService，造成 UI/CLI 卡顿与超时。
3. **Legacy 历史包袱**：早期遗留的 `LegacyResult`、旧版 CLI 执行壳和散落的临时状态导致错误传递与测试基线脆弱。

参考 [deepseek-ai/deepseek-harness](https://github.com/deepseek-ai/deepseek-harness)（Honeycomb 架构）与 [earendil-works/pi](https://github.com/earendil-works/pi)（Pi Agent 微内核与模块化 Harness）的设计，项目启动了**全生命周期中最大规模、施工量最重的一次全系统运行时大重构**。近期所有的后端施工与边界收口，本质上都是在固化和完成这次跨越。

## 决策

1. **下沉统一的扩展微内核（Extension Kernel）**：
   - 建立底层的 `extension_kernel` 核心模块，统管所有扩展（无论是 Agent 还是 Conversation Adapter）的包身份（`PackageIdentity`）、分发渠道（`System`, `Binary`, `Npx`, `Uvx`）、生命周期状态机（`LifecycleTaskCoordinator`）、信任准入（`TrustGate`）与沙箱探活（`ProbeSpec`）。
   - **双市场同构统一**：Agent 市场与 Adapter 市场作为微内核之上的两个领域特化投影，底层共享 100% 同构的执行、安装与版本治理基础设施。
2. **非阻塞 TaskRuntime 与全局锁解耦**：
   - 所有耗时 I/O 必须全面接入 `TaskRuntime` / `BackgroundTaskRegistry` 作为后台任务执行，Tauri 命令与 Engine 快速返回 Task Snapshot，严禁在主请求流程中阻塞 `await` 外部 I/O。
   - 废除全局大锁，引入基于路径与 Profile 字典序排序的细粒度锁（`Keyed Locks / PlanScopeGuard`），杜绝死锁与阻塞。
3. **全面替代并清理 Legacy 架构**：
   - 彻底废除并删除旧版 `LegacyResult`、旧 Agent CLI 执行栈与零散配置，全量迁移至类型化 `AppError`、统一 `AppResult` 与 SQLite 权威存储。

## 备选方案

### 继续修补各自独立的双市场体系

- 缺点：底层维护两套平行的安装分发逻辑与进程生命周期，随着生态扩展，维护成本呈指数级上升；全局锁导致系统在高并发/大 I/O 场景下必然卡死。
- 结论：坚决否决。

## 后果与替代关系

- **全面替代（Supersedes）**：本决策作为系统最终集大成者，正式替代了早期单体粗粒度的 Legacy 执行结构、旧版独立命令壳与全局阻塞锁。
- 系统完成了从“分散特性拼装”向“工业级可扩展微内核平台”的决定性跨越。
- 为 AssetIWeave 后续接入更多异构 Agent 框架与海量会话数据提供了极具韧性的性能与安全基座。
