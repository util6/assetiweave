# A-R02：接入启动配置并删除散落 env 解析

**Depends:** A-R01
**Contracts:** C-BASE、C-CONFIG、C-STORAGE
**Outcome:** Desktop、Engine、MCP 桥及应用内部共用同一个启动配置语义，SQLite settings 不变。

## 执行规则

- 状态：`PLANNED`。本文件只授权本卡范围；实施前读取 `../00-execution-router.md`、本卡 Contract IDs、`../02-dependencies.md` 和 `../05-playbook.md`。
- 版本/features 只取依赖锁定表；本卡列出的新签名是**待创建接口**，不是已实现事实。
- 原有正确行为先 characterization green；随后运行本卡 adoption/deletion guard 得到 red，再替换实现。编译错误只可证明新接口未创建，不能冒充行为回归证据。
- 测试只用临时目录、内存库或 loopback fixture；不读写真实用户数据库。筛选测试先 `-- --list` 确认命中；零测试不算 green。
- 结束只交付本卡证据与下一张 ready 卡；任务二由独立目录的入场门控制。

## 文件与接缝

- Modify：`src-tauri/src/backend/runtime/config.rs`、`runtime/app_runtime.rs`、`runtime/mod.rs`、`path_utils.rs`、`app_settings.rs`、`logs.rs`、`application/system.rs`；`src-tauri/src/lib.rs`；`src-tauri/src/adapters/engine/policy.rs`。
- Test：`runtime/config.rs::tests`、`runtime/tests.rs`、`app_settings.rs` 内联测试、`src-tauri/src/adapters/engine/policy.rs` 内联测试；`adapters/engine/transport.rs` 现有 policy 测试仅验证，不扩大修改。
- Create：无。注册字段放既有 AppRuntime，不另建全局配置容器。

## Consumes / Produces

Consumes：A-R01 的 `RuntimeConfig::from_environment() -> AppResult<RuntimeConfig>`；已有 `current_process_runtime() -> Option<Arc<AppRuntime>>`。

Produces（创建）：

```rust
pub(crate) fn runtime_config() -> AppResult<std::sync::Arc<RuntimeConfig>> {
    if let Some(runtime) = super::current_process_runtime() {
        return Ok(runtime.config());
    }
    RuntimeConfig::from_environment().map(std::sync::Arc::new)
}
// AppRuntime 的新增字段：config: Arc<RuntimeConfig>
// AppRuntime 的新增方法：
pub(crate) fn config(&self) -> Arc<RuntimeConfig> {
    Arc::clone(&self.config)
}
```

`AppRuntime::bootstrap(db_path: PathBuf, role: RuntimeRole) -> AppResult<Arc<Self>>` 保持签名；启动时加载配置并以显式 `db_path` 参数覆盖 config.db_path，存为快照。测试 builder 从同一纯解析器拿默认，显式注入临时 db_path。进程 runtime 安装之后的 helper 均读该快照；安装前才临时解析，避免测试污染新的 OnceLock。

## 步骤

- [ ] 跑现有 settings 首次导入和 Engine policy 测试，保存 green，特别确认 config.json 不会在 SQLite 有行时再覆盖设置。
- [ ] 在 config 测试模块加入 source guard，确认旧读取存在而 red：

```rust
#[test]
fn startup_consumers_do_not_parse_environment_again() {
    let sources = [
        include_str!("../path_utils.rs"),
        include_str!("../logs.rs"),
        include_str!("../app_settings.rs"),
    ];
    let old_read = concat!("var_os(\"ASSETIWEAVE_", "DB_PATH\")");
    let old_home = concat!("var(\"ASSETIWEAVE_", "HOME\")");
    assert!(sources.iter().all(|source| !source.contains(old_read)));
    assert!(sources.iter().all(|source| !source.contains(old_home)));
}
```

- [ ] 给 AppRuntime 添加快照字段和 getter；两个生产/测试构造分支均赋值。使用 `let mut config = RuntimeConfig::from_environment()?; config.db_path = db_path.clone();`，避免改 bootstrap 的调用协议。
- [ ] 改原 helper 的**取值部分**，目录创建仍在原需要副作用的入口。例如：

```rust
pub(crate) fn app_db_path() -> AppResult<PathBuf> {
    let path = crate::backend::runtime::config::runtime_config()?.db_path.clone();
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        std::fs::create_dir_all(parent)?;
    }
    Ok(path)
}
```

- [ ] `app_config_dir`、`memory_legacy_archive_root`、`get_log_dir` 从 config 读 home/log；保留各自目录后缀和文件创建语义。Engine/Team-MCP/Recall-MCP 的 DB 环境读取统一调用 `app_db_path`；作用域 credential 环境读取不动。
- [ ] `policy::authorize` 只从 config 获取 policy_path，继续每次读取策略文件并执行旧 allow/deny/risk 规则；启动快照只冻结路径，不改变文件内容更新行为。
- [ ] 删除 `application/system.rs` 测试专用重复 `engine_db_path` 解析，测试调用相同 helper；补同一注入db路径在 runtime getter/helper中一致的测试。
- [ ] 测试仍要临时修改环境的，延用项目既有串行 env guard，不把环境修改迁入异步并行测试；优先改为 A-R01 纯函数测试。

## 删除清单

四个已知启动 env 的调用点解析退出 `path_utils/app_settings/logs/lib/policy/system`；唯一环境采集留在 runtime/config.rs。`app_settings` 的 SQLite load/import/canonicalize 和 legacy文件迁移逻辑保留。HOME/DB目录名称和副作用不可借机重设计。

## 验证与停止条件

```bash
cargo test -p assetiweave --lib startup_consumers_do_not_parse_environment_again -- --nocapture
cargo test -p assetiweave --lib runtime::config::tests
cargo test -p assetiweave --lib app_settings
cargo test -p assetiweave --lib engine::policy
cargo test -p assetiweave --lib engine::transport -- --test-threads=1
cargo fmt --all -- --check
```

成功：guard 命中且通过；SQLite 已有值优先/首次导入/诊断命令policy豁免/错误policy测试不退化；显式临时db仍覆盖默认。停止：新快照会冻结本应每次变化的用户设置，或调用者依赖测试期间动态修改全局路径却未隔离；先报告调用者和测试，不增加第二套配置权威。

[官方 API：config](https://docs.rs/config/latest/config/)
