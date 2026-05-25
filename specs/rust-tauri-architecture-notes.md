# Rust/Tauri 分层范式与 Spring 对比

## 1. 背景

Rust 和 Tauri 没有像 Spring 那样由官方框架强制规定的 `Controller`、`Service`、`Mapper`、`Repository` 分层范式。Rust 项目更依赖：

- Cargo workspace 和 crate 边界。
- module 文件组织。
- trait 抽象。
- 显式依赖传递。
- 类型系统和错误类型。
- 领域逻辑与外部 adapter 的隔离。

因此，大型 Rust/Tauri 项目的“规范”通常不是统一命名，而是统一边界：入口薄、核心纯、外设隔离、持久化集中。

## 2. 大型 Rust 项目的常见范式

### 2.1 Cargo Workspace + 多 crate 边界

大型 Rust 项目常用 workspace 把职责拆到不同 crate 中，而不是把所有代码放在一个应用 crate 里。

典型例子：

- Tauri：核心、runtime、codegen、utils、CLI、plugin 分开。
- rust-analyzer：LSP 入口、IDE API、语义模型、虚拟文件系统、项目模型分开。
- Zed：按编辑器、工作区、语言、UI、协作、扩展等产品领域拆 crate。
- Lapce：按 app、core、proxy、rpc 等运行边界拆分。

这种模式对应 Java 里的多 module Maven/Gradle 项目，但 Rust 的 crate 边界更强，因为可见性、编译依赖和 API 暴露都由 crate 明确约束。

### 2.2 Ports and Adapters / Clean Architecture

Rust 项目常见做法是把核心领域逻辑和外部系统隔离：

- Core/domain 不知道 UI、Tauri、SQLite、文件管理器。
- Store/repository 负责数据库。
- Platform adapter 负责操作系统差异。
- Command/API 层负责接收外部请求。

这和 Clean Architecture 的思想接近：

```text
外层：Tauri commands / SQLite / OS / file system
中层：application service / use case
内层：domain model / planner / scanner / rules
```

依赖方向应该尽量从外层指向内层，而不是内层反向依赖外层。

### 2.3 Feature/Domain Module 风格

一些产品型 Rust 项目会按业务领域拆分，而不是按技术层拆分。例如：

```text
crates/editor
crates/workspace
crates/language
crates/project
crates/ui
crates/collab
```

这种方式适合功能巨大、领域复杂的产品。每个领域模块内部再分 service、store、types、commands。

### 2.4 Thin Entry Point

Rust/Tauri 项目通常希望入口文件很薄：

```text
main.rs / lib.rs
```

只负责：

- 初始化配置。
- 创建应用状态。
- 注册插件。
- 注册 command handler。
- 启动应用。

不应该在入口文件里写 SQL、扫描、部署、业务规则、平台命令。

## 3. 与 Spring 分层的对照

| Spring 概念 | Rust/Tauri 常见对应 | 说明 |
|---|---|---|
| Controller | `commands/` | Tauri command 是前后端 IPC 入口，类似 Controller。 |
| Service | `domain/`、`application/`、`planner/`、`scanner/`、`executor/` | 业务规则和用例编排。 |
| Mapper / DAO | `store/`、`repository/`、`persistence/` | 数据库访问，SQL 集中在这里。 |
| Entity | `models/`、core crate 中的 domain types | 核心领域模型。 |
| DTO | `dto.rs`、`api_types.rs` | 前端入参/出参，不一定等同领域模型。 |
| Config | `config.rs`、`defaults.rs` | 默认配置、路径模板、Profile 模板。 |
| Utils | `path_utils.rs`、`platform/` | 路径、OS 差异、文件管理器打开等。 |
| Exception | `error.rs` + `thiserror` | Rust 通常用 `Result<T, E>` 和枚举错误。 |
| Dependency Injection | 显式构造、`AppState`、trait | 没有 Spring 容器，依赖通常显式传递。 |

## 4. Rust 和 Spring 的关键差异

### 4.1 没有注解驱动容器

Spring 依赖注解和 IoC 容器：

```java
@Service
@Repository
@Autowired
```

Rust 通常不用运行时容器。依赖关系通过构造函数、结构体字段、函数参数、trait 显式表达。

```rust
pub struct AppServices {
    pub store: SqliteStore,
}
```

Tauri 中常见做法是把共享状态注册到 app：

```rust
.manage(AppState { db_path, lock })
```

command 中再取出：

```rust
fn list_assets(state: tauri::State<AppState>) -> Result<Vec<Asset>, String>
```

### 4.2 trait 类似接口，但不应过度抽象

Java 常常先定义 interface：

```java
interface AssetService {}
class AssetServiceImpl implements AssetService {}
```

Rust 可以用 trait：

```rust
pub trait AssetStore {
    fn list_assets(&self) -> AppResult<Vec<Asset>>;
}
```

但 Rust 社区通常不鼓励为了“像接口”而提前抽象。只有当存在以下需求时，trait 才更有价值：

- 多个实现。
- 测试需要 mock。
- crate 边界需要隐藏具体类型。
- 插件化或可替换 adapter。

### 4.3 错误处理显式

Spring 常通过异常传播。Rust 通常通过 `Result<T, E>`：

```rust
pub type AppResult<T> = Result<T, AppError>;
```

更规范的错误类型：

```rust
#[derive(thiserror::Error, Debug)]
pub enum AppError {
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("{0}")]
    Message(String),
}
```

Tauri command 最外层可以再把错误转成前端可接收的字符串。

### 4.4 crate 是强边界

Java package 更多是组织方式，Rust crate 同时也是编译、可见性和依赖边界。

例如：

```text
crates/assetiweave-core
src-tauri
```

理想状态：

- `assetiweave-core` 不依赖 Tauri。
- 核心 scanner/planner 逻辑可以被 CLI、测试、桌面 app 复用。
- `src-tauri` 只做桌面集成和外部 adapter。

## 5. Tauri 应用推荐分层

对于 AssetIWeave 这类本地优先桌面应用，推荐结构：

```text
src-tauri/src/
  lib.rs
  app_state.rs
  dto.rs
  error.rs
  commands/
    mod.rs
    assets.rs
    sources.rs
    profiles.rs
    plans.rs
  store/
    mod.rs
    sqlite.rs
    schema.rs
    sources.rs
    assets.rs
    profiles.rs
    deployment_state.rs
  domain/
    mod.rs
    scanner.rs
    classifier.rs
    planner.rs
    executor.rs
  platform/
    mod.rs
    paths.rs
    reveal.rs
  defaults.rs
```

### 5.1 `lib.rs`

只做启动和装配：

- 初始化数据库路径。
- 初始化 `AppState`。
- 注册 Tauri plugins。
- 注册 commands。
- 启动应用。

不放 SQL、不放业务规则、不放扫描逻辑。

### 5.2 `commands/`

类似 Spring Controller。

职责：

- 接收前端参数。
- 调用 store/domain/platform。
- 返回 DTO。
- 做少量错误转换。

不应该：

- 写 SQL。
- 遍历文件系统。
- 生成复杂部署计划。
- 直接包含平台分支。

### 5.3 `store/`

类似 Mapper/Repository。

职责：

- SQLite schema。
- migration。
- SQL 查询。
- 数据对象和数据库行转换。

SQL 应集中在这里，避免散落在 command 或 domain 中。

### 5.4 `domain/`

类似 Service，但更强调纯业务逻辑。

职责：

- 扫描策略。
- 资产分类。
- Profile 规则匹配。
- 部署计划生成。
- 部署执行编排。

domain 层应尽量不依赖 Tauri，这样更容易测试。

### 5.5 `platform/`

负责操作系统差异。

职责：

- home path 展开和缩写。
- macOS Finder reveal。
- Windows Explorer reveal。
- Linux xdg-open。
- symlink 差异。

这些分支不应该散在 planner 或 command 中。

### 5.6 `defaults.rs`

负责默认模板：

- 默认 Source。
- 默认 Profile。
- 默认 include/exclude globs。
- 默认安全策略。

这样默认路径和模板不会硬编码在 SQLite store 或 Tauri command 中。

## 6. AssetIWeave 的建议边界

当前项目可以按下面方式理解：

```text
commands = API 层
store = SQLite 持久化层
domain = 业务规则层
platform = 操作系统适配层
defaults = 配置模板层
crates/assetiweave-core = 可复用核心模型和纯逻辑
```

对应 Spring 心智模型：

```text
Controller  -> commands
Service     -> domain
Mapper      -> store
Config      -> defaults
Utils       -> platform
Entity      -> core models
DTO         -> dto
```

## 7. 实践规则

1. `lib.rs` 控制在 50 到 100 行左右。
2. SQL 只出现在 `store/`。
3. Tauri command 只做薄封装。
4. 扫描、分类、计划生成和部署执行不依赖 Tauri。
5. OS 分支只出现在 `platform/`。
6. 默认路径和默认模板只出现在 `defaults.rs`。
7. 只有在需要 mock、多实现或隐藏实现细节时才引入 trait。
8. 用 `thiserror` 建统一错误类型，少用裸 `String`。
9. DTO 和 domain model 可以先共用，但边界变复杂后要拆开。
10. 核心逻辑优先放进独立 crate，方便后续 CLI 或测试复用。

## 8. 简短结论

Rust/Tauri 的大型项目范式不是“官方 Spring 分层”，而是：

```text
薄入口 + command adapter + domain service + store repository + platform adapter + crate 边界
```

它和 Spring 的目标一致：降低耦合、明确职责、方便测试和演进。区别在于 Rust 更倾向显式依赖、模块边界和编译期约束，而不是注解、反射和运行时容器。
