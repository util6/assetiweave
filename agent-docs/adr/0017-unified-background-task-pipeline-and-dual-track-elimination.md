# 0017: 统一后台任务流水线引擎、Task Journal 持久化与长耗时双轨制彻底消灭

> **状态**：已接受
> **决策日期**：2026-10-08
> **产品规格**：[Issue #77](https://github.com/util6/assetiweave/issues/77)
> **关联专项**：[Issue #33](https://github.com/util6/assetiweave/issues/33)
> **取代**：分散在 `desktop/src/adapters/tauri/background_tasks/` 下的各类私有任务快照与私有更新事件（`source-scan-task-updated`, `batch-mount-task-updated` 等）

## 背景

在 AssetIWeave 系统中，执行时间超过 1 秒的操作（包括来源目录扫描、文件哈希校验、批量符号链接挂载、Git 远程拉取、Agent 安装、全文索引重建等）长期存在严重架构断层与双轨制混淆：
1. **双轨制与私有事件泛滥**：系统并存着未完成的统一任务中心与各类领域私有任务（如 `SourceScanTaskSnapshot`、`BatchMountTaskSnapshot`）及私有事件（如 `source-scan-task-updated`）。开发者与 AI Agent 频繁绕过任务中心或依赖 `TaskContext::untracked()` 逃逸后门。
2. **高频事件洪峰导致界面卡死**：数值进度上报未做节流，扫描几千个文件时高频全量广播完整快照，造成 Tauri IPC 与前端 React 状态树严重拥塞与渲染假死。
3. **纯内存运行态与数据不一致（僵尸任务）**：任务状态仅停留在内存。当应用遭遇崩溃、强制退出或重启时，内存任务蒸发，但底层 SQLite 业务状态留存脏数据（如 `last_scan_status = 'pending'`），且无历史可查。
4. **CLI 与桌面端体验撕裂**：CLI 客户端执行命令时，因后台任务直接 fire-and-forget 导致命令过早退出，破坏了 POSIX 退出码与终端流式进度体验。

## 决策

1. **唯一业务收口与编译器门禁**：
   - 彻底废除生产代码中的 `TaskContext::untracked()`，限制其仅在 `#[cfg(test)]` 可用；内部无状态任务必须显式使用 `TaskContext::detached(reason)` 留下审计理由。
   - `AppService` 统一提供强类型流水线门面 `spawn_pipeline_task`，禁止业务层直接裸起线程执行长耗时写操作。
   - 阶段 ID 强制使用 `pub const` 常量（如 `stages::SCAN_FILES`），严禁任意拼写字符串引发阶段状态机漂移。

2. **SQLite Task Journal 表（一表三用与动静分离）**：
   - 建立轻量级追加写 `task_journal` 表，严格只记录生命周期关键帧（任务创建、阶段进出、终态结算、停机中断），**严禁高频数值进度写盘**。
   - 一表三用：
     - **冷启动自愈**：应用启动时运行 `reconcile_orphaned_tasks_on_startup`，将上次未完成任务收敛为 `interrupted_by_shutdown`，同步修复业务脏数据。
     - **任务保留与历史中心**：任务中心直接查 Journal 获取最近 30 天历史。
     - **自动重跑依据**：记录幂等任务重派参数。
   - 引入 4 步优雅停机序列：停止接单 -> 取消并超时排空 -> 写入 `interrupted_by_shutdown` 帧 -> 关闭 SQLite 连接池。

3. **双轨事件信封与单调保序节流**：
   - 广播事件信封显式标记 `pub enum TaskChangeClass { Structural, Cosmetic }`。
   - `Cosmetic` 帧（数值进度）实行 150ms 窗口节流；`Structural` 帧（阶段变迁、终态、故障）立刻旁路发出。
   - **保序规约**：发出 `Structural` 帧前，必须先将节流窗口中挂起的 `Cosmetic` 帧冲刷（Flush），防止出现先收到新阶段再收到旧阶段 98% 的时间线倒挂。

4. **双端协议同构（CLI 阻断等待与桌面异步视图）**：
   - 桌面端返回 `TaskView` / `TaskSnapshot` 并监听 `task-updated`。
   - Go CLI 针对同步类命令通过 stdio 协议执行阻塞等待（`wait_blocking`），流式打印阶段行与单行刷新进度条，保持标准退出码。

5. **前端领域解耦自动刷新机制**：
   - 前端建立 `taskInvalidationRegistry`，领域模块（如 Catalog）自注册对特定 TaskKind 的 QueryKey 刷新回调，任务中心在收到终态 `Structural` 帧时统一分发触发，彻底解耦。

6. **垂直改穿与旧轨物理删除**：
   - 以数据源扫描（`scan_sources`）为黄金切片端到端切穿，接入三阶段标准流水线。
   - **物理删除** `SourceScanTaskSnapshot` DTO 与 `SOURCE_SCAN_TASK_UPDATED_EVENT` 字符串常量，确保代码库只保留统一真理源。

## 范围界限（明确不做）

- **不做** 任务暂停与恢复（Pause / Resume）。
- **不做** 线程优先级抢占（Priority Preemption）。
- **不做** 算命式剩余时间预测（ETA Prediction）。

## 后果

- 彻底根除长任务双轨制与私有更新事件，新任务开发有标准范式可抄，Agent 与开发者均受编译器与常量约束。
- 解决高频读盘与扫描时的 IPC 洪峰卡死问题，界面响应丝滑。
- 系统崩溃或重启后具备 100% 确定性的自愈与数据一致性，消灭永久 pending 僵尸任务。
