# 0001: 应用程序数据库统一使用 SQLx 迁移与持久化

> 状态：已接受
> 决策日期：2026-06-21
> 决策证据：`f5633e4f87c95fc0c8d25471b26d11238f0d4ce8`
> 记录日期：2026-06-21

## 背景

AssetIWeave 使用本地 SQLite 数据库作为应用目录，用于管理源目录（sources）、资产（assets）、Profile、部署状态（deployment state）、导航（navigation）、应用快捷方式（app shortcuts）、资产分组（asset groups）、会话记录（conversation records）、Web 记录与同步元数据。

最初的后端将仓储（repository）逻辑与临时的 rusqlite 辅助函数以及一个仅用于测试的庞大 `INIT_SCHEMA` 字符串混杂在一起。这极易导致架构漂移（schema drift）：生产环境初始化、测试用例以及未来的 Agent 可能会各自通过不同路径来创建数据表。

后端目前需要一个具有唯一权威的 Schema 生命周期：

- 全新的数据库必须通过有序的迁移文件来创建；
- 现存的迁移前旧数据库必须平滑接管，不能丢失任何数据行；
- 应用自有的持久化路径必须统一使用共享的 Rust store 层；
- 前端与 Go CLI 调用方必须通过 Tauri 命令或 Engine 接口交互，严禁直接读写 SQLite。

## 决策

统一使用 SQLx 0.9 搭配 SQLite，并将 SQLx 迁移作为应用程序数据库的唯一权威路径。

具体规则：

- 应用程序数据库初始化统一经由 `Database::open_initialized`，该方法会执行来自 `src-tauri/migrations` 的内嵌 SQLx 迁移器。
- Schema 变更必须在 `src-tauri/migrations` 中表示为带时间戳的文件；严禁重新引入手写的 Schema 初始化常量（如 `INIT_SCHEMA`）。
- 应用自有的 Repository 模块使用 `SqlitePool`、SQLx 查询以及 SQLx 事务。新 Repository 函数仅在存在活跃迁移边界期间可带有 `_sqlx` 后缀；当旧路径移除后，SQLx 实现即为规范标准实现。
- Repository 测试必须针对通过 `Database` 创建的临时数据库来验证基于 SQLx 的行为。
- 剩余的 `rusqlite` 使用仅允许在非应用自有 Repository 写入的明确边界内存在：
  - 需要检查外部 Codex/OpenCode SQLite 文件的外部会话源读取器；
  - 仅用于测试的迁移接管验证或备份可读性检查。

## 备选方案

### 为 Repository 保留 rusqlite 并单独添加迁移机制

- 优点：依赖改动较小。
- 缺点：保留了两种数据库访问风格，且异步服务代码必须持续桥接同步数据库调用。
- 结论：否决。目标是建立统一的后端持久化架构，而不仅仅是一个迁移执行器。

### 为测试保留手写的 Schema 初始化器

- 优点：内存测试搭建快速。
- 缺点：创建了第二个 Schema 事实来源，掩盖了迁移漂移问题。
- 结论：否决。测试必须验证与应用程序完全相同的初始化路径。

### 将所有 SQLite 访问迁移到 SQLx（包括外部 Codex/OpenCode 读取器）

- 优点：依赖图中只有一个 SQLite crate。
- 缺点：外部读取器代码不是应用自有的持久化逻辑，目前依赖于对第三方 SQLite 布局的动态反射与检查。
- 结论：暂缓。如果读取器代码后续需要异步 I/O，或者将完全移除 rusqlite 作为依赖策略，可单独重新评估。

## 后果

- Schema 审查即迁移审查。
- 应用程序与测试初始化使用相同的迁移器，消除了 Schema 漂移。
- Repository 代码可以与支持后台运行的 Tauri 工作流及 Engine 调用共享连接池化异步数据库访问。
- 未来任何新增表、列、索引、触发器或 FTS 结构的改动，都必须包含迁移文件与回归测试。
- 代码库在非 Repository 边界仍可能包含 rusqlite，但该用法必须保持隔离，不得创建或修改应用目录。
