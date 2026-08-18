# ADR-008：AppRuntime 与锁粒度

- 状态：Accepted
- 日期：2026-08-18

## 背景

Tauri 命令与 Engine dispatch 过去会重复打开数据库、创建 tokio Runtime、seed 默认数据并恢复 Agent 注册表；Tauri 还以 `AppState.lock` 把无关操作串行化。

## 决策

1. 每个进程只创建一个 `AppRuntime`。它持有迁移后的 SQLite 连接池、唯一同步桥、请求上下文快照、Agent runtime、按资源键的锁表、TaskRuntime 与关闭状态。
2. ResidentHost（Tauri）与 OneShot（Engine）共用同一 bootstrap。只有 ResidentHost 启动跨调用 dispatcher；OneShot 只保留进程内任务能力。
3. `AppService` 是 `Arc<AppRuntime>` 的轻量请求门面，构造不执行 I/O。生产请求不得再调用 `Database::open_initialized`。
4. 数据库写一致性由事务负责；部署计划以排序后的路径/profile 键集合获取 keyed lock；扫描使用任务去重；普通查询不获取业务锁。
5. `RwLock`/`Mutex` guard 不跨 `.await`，长任务使用独立连接或短事务，不持有全局租户锁执行阻塞 I/O。
6. 内部错误逐步迁移到结构化 `AppError`；现有 wire 错误文本保持兼容，新增错误码通过边界 DTO 暴露。

## 后果

请求初始化从完整打开链路变成 O(1) 快照绑定；应用关闭需要先停止任务准入、完成 dispatcher drain，再收敛任务和关闭连接池。旧的 `Database::open_initialized` 保留给测试与迁移工具，生产入口统一走 `AppRuntime::bootstrap`。

## 回滚

`AppService::from_runtime` 与测试专用的 `open_with_db_path` 可在迁移期间并存；任何单类命令接线都可以独立回退。
