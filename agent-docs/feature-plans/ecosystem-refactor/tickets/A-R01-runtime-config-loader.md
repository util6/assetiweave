# A-R01：用 config 建立四字段启动配置解析器

**Depends:** A00
**Contracts:** C-BASE、C-CONFIG、C-ERROR
**Outcome:** 四个既有启动环境变量有一个可纯函数测试的解析入口；本卡不切换调用者。

## 执行规则

- 状态：`PLANNED`。本文件只授权本卡范围；实施前读取 `../00-execution-router.md`、本卡 Contract IDs、`../02-dependencies.md` 和 `../05-playbook.md`。
- 版本/features 只取依赖锁定表；本卡列出的新签名是**待创建接口**，不是已实现事实。
- 原有正确行为先 characterization green；随后运行本卡 adoption/deletion guard 得到 red，再替换实现。编译错误只可证明新接口未创建，不能冒充行为回归证据。
- 测试只用临时目录、内存库或 loopback fixture；不读写真实用户数据库。筛选测试先 `-- --list` 确认命中；零测试不算 green。
- 结束只交付本卡证据与下一张 ready 卡；任务二由独立目录的入场门控制。

## 文件与接缝

- Create：`src-tauri/src/backend/runtime/config.rs`，配置类型、纯解析、环境装配和内联测试。
- Modify：`src-tauri/src/backend/runtime/mod.rs`，声明 `pub(crate) mod config;`；`src-tauri/Cargo.toml`、`Cargo.lock`，只添加锁定的 config。
- Read：`src-tauri/src/backend/path_utils.rs:12–28,58–67`、`backend/app_settings.rs:186–200`、`backend/logs.rs:196–207`、`src-tauri/src/adapters/engine/policy.rs:50–59`。
- Test：新 `runtime/config.rs::tests`；本卡不启动 AppRuntime、不迁移 SQLite。

## Consumes / Produces

Consumes（已有）：`AppResult<T> = Result<T, AppError>`；`dirs::home_dir() -> Option<PathBuf>`、`dirs::data_dir() -> Option<PathBuf>`。

Produces（创建，供 A-R02）：

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RuntimeConfig {
    pub(crate) home_dir: std::path::PathBuf,
    pub(crate) db_path: std::path::PathBuf,
    pub(crate) log_dir: std::path::PathBuf,
    pub(crate) policy_path: Option<std::path::PathBuf>,
}
#[derive(Debug, Clone)]
pub(crate) struct RuntimeConfigDefaults {
    pub(crate) home_dir: std::path::PathBuf,
    pub(crate) data_dir: std::path::PathBuf,
}
impl RuntimeConfig {
    pub(crate) fn from_env_map(
        env: &std::collections::BTreeMap<String, std::ffi::OsString>,
        defaults: &RuntimeConfigDefaults,
    ) -> crate::backend::runtime::AppResult<Self>;
    pub(crate) fn from_environment() -> crate::backend::runtime::AppResult<Self>;
}
```

签名块是契约声明；实现时把函数体写入上述 impl，不把分号声明复制成可编译实现。

## 决定好的行为

只读取 `ASSETIWEAVE_HOME`、`ASSETIWEAVE_DB_PATH`、`ASSETIWEAVE_LOG_DIR`、`ASSETIWEAVE_POLICY_PATH`。HOME 的 UTF-8 值 trim 后空则未设置；DB_PATH 仅空 OsString 未设置，非空（包括空格）是原始路径；LOG_DIR/POLICY_PATH 只要键存在就保留原值，包括空串，不能把空 policy 变成无策略。PathBuf 不自动 canonicalize，不要求路径已存在。保留非 UTF-8 OsString 路径：这类值在 config 反序列化后按原始 OsString 覆盖对应 PathBuf，避免 lossy 转换。

默认值分别为 `defaults.home_dir/.assetiweave`、`defaults.data_dir/AssetIWeave/app.db`、`defaults.data_dir/AssetIWeave/logs`、`None`。HOME 不隐式重定位 DB/LOG，这是现有目录语义。环境优先于默认值。解析不创建目录。`from_environment` 只采集四个键以及 dirs 默认目录；凭据、PATH、RUST_LOG、产品设置不进入结构。

## 步骤

- [ ] 记录旧四个入口的缺省/覆写行为；先补纯路径期望表，确认 characterization green。
- [ ] 添加新接口测试，运行确认因 `RuntimeConfig` 未定义而 red：

```rust
#[test]
fn explicit_db_override_does_not_relocate_log_dir() {
    use std::{collections::BTreeMap, ffi::OsString, path::PathBuf};
    let defaults = RuntimeConfigDefaults {
        home_dir: PathBuf::from("/fixture/home"),
        data_dir: PathBuf::from("/fixture/data"),
    };
    let env = BTreeMap::from([
        ("ASSETIWEAVE_HOME".into(), OsString::from("/fixture/custom")),
        ("ASSETIWEAVE_DB_PATH".into(), OsString::from("/fixture/test.db")),
        ("ASSETIWEAVE_TEAM_TOOL_CREDENTIAL".into(), OsString::from("secret")),
    ]);
    let value = RuntimeConfig::from_env_map(&env, &defaults).unwrap();
    assert_eq!(value.db_path, PathBuf::from("/fixture/test.db"));
    assert_eq!(value.home_dir, PathBuf::from("/fixture/custom"));
    assert_eq!(value.log_dir, PathBuf::from("/fixture/data/AssetIWeave/logs"));
    assert_eq!(value.policy_path, None);
}
```

- [ ] 用 config 的 builder + typed deserialize 替代手写优先级。中间 DTO 只放四个 `Option<String>`，不把整个环境导入 config：

```rust
#[derive(serde::Deserialize, Default)]
struct Utf8Overrides {
    home_dir: Option<String>,
    db_path: Option<String>,
    log_dir: Option<String>,
    policy_path: Option<String>,
}
let mut builder = config::Config::builder();
for (env_key, field) in [
    ("ASSETIWEAVE_HOME", "home_dir"),
    ("ASSETIWEAVE_DB_PATH", "db_path"),
    ("ASSETIWEAVE_LOG_DIR", "log_dir"),
    ("ASSETIWEAVE_POLICY_PATH", "policy_path"),
] {
    if let Some(raw) = env.get(env_key).and_then(|value| value.to_str()) {
        let value = if env_key == "ASSETIWEAVE_HOME" { raw.trim() } else { raw };
        if (env_key == "ASSETIWEAVE_HOME" || env_key == "ASSETIWEAVE_DB_PATH") && value.is_empty() {
            continue;
        }
        builder = builder.set_override(field, value)
            .map_err(crate::backend::runtime::AppError::external)?;
    }
}
let parsed: Utf8Overrides = builder.build()
    .map_err(crate::backend::runtime::AppError::external)?
    .try_deserialize()
    .map_err(crate::backend::runtime::AppError::external)?;
```

- [ ] 显式构造四个结果字段：`parsed` 的 Some 转 PathBuf、None 取上述默认；随后只对四个已知键的非 UTF-8 非空 OsString 覆写。补 HOME 空白/DB原始空格路径/空policy仍为Some空路径、独立HOME、policy缺省、Unix非UTF8、解析不创建目录测试。
- [ ] 加 adoption guard：`include_str!("config.rs")` 必须含 `config::Config::builder`，防止新入口仍全部手写；接口测试与 guard 全绿。

## 删除与验证

本卡不删除旧调用入口；它们只在 A-R02 切换后删除，A-R01 的未接入状态不等于任务一完成。

```bash
cargo test -p assetiweave --lib runtime::config::tests -- --list
cargo test -p assetiweave --lib runtime::config::tests -- --nocapture
cargo fmt --all -- --check
```

成功：至少五个新解析测试通过；输出不含敏感环境字段；只新增本卡依赖。停止：默认目录语义与 C-CONFIG 不一致、执行者试图把 SQLite settings 或凭据并入配置时，报告差异，不自行扩大配置模型。

[官方 API：config](https://docs.rs/config/latest/config/)
