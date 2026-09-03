# A-R05：validator 接管通用 DTO 字段校验

**Depends:** A-R04
**Contracts:** C-BASE、C-ERROR、C-STORAGE
**Outcome:** 先迁移 Team 名称/成员输入及 Agent catalog 通用字段，删除对应手写重复校验，保留业务规则。

## 执行规则

状态：`PLANNED`。先读总入口、本卡 Contract IDs、`../02-dependencies.md`、`../05-playbook.md`。一轮只做本卡。原有正确行为先 characterization green；随后新增 adoption/deletion guard 得到 red，再迁移。筛选测试先 `-- --list`，零测试不算 green。只用临时目录/内存库/loopback fixture；本卡不授权插件架构或真实用户数据操作。

## 文件

- Modify：`src-tauri/src/backend/models/team.rs`、`store/team_repo.rs`、`agent_market/types.rs`、`runtime/error.rs`；`src-tauri/Cargo.toml`、`Cargo.lock`。
- Create：`src-tauri/src/backend/validation.rs`；Modify `backend/mod.rs` 导出该模块。
- Test：`validation.rs::tests`、`models/team.rs` 内联测试、`store/team_repo.rs` 既有临时库测试、`agent_market/types.rs` 内联测试。

## 接口

Consumes：`AppError::Validation(String)`，现有 DTO JSON字段不变。

Produces（创建）：

```rust
pub(crate) fn validate_non_blank(value: &str) -> Result<(), validator::ValidationError> {
    if value.trim().is_empty() {
        Err(validator::ValidationError::new("required"))
    } else { Ok(()) }
}
pub(crate) fn validate_max_120_bytes(value: &str) -> Result<(), validator::ValidationError> {
    if value.len() > 120 {
        Err(validator::ValidationError::new("length_bytes"))
    } else { Ok(()) }
}
pub(crate) fn validation_error(errors: validator::ValidationErrors) -> AppError;
```

最后一个函数在 runtime/error.rs 创建，仅映射受控的字段名/错误code，不序列化 `ValidationError.params` 的原始输入值。普通 DTO字段未知错误回落 `validation_error`；既有 Team “name empty”和member index文案仍在其调用边界按字段定位保持。

## 步骤

- [ ] 先运行 Team roster 和 Agent catalog 现有验证测试。记录：Team允许无最大名称长度、必须恰好一个Leader；Catalog display_name目前120 **字节**，不是120字符。
- [ ] 写纯字段测试及 derive约束测试得到red：

```rust
#[test]
fn team_name_field_uses_validator_without_changing_whitespace_semantics() {
    use validator::Validate;
    let input = crate::backend::models::CreateTeamInput {
        id: None, name: "  ".into(), description: None, members: vec![],
    };
    let errors = input.validate().unwrap_err();
    assert!(errors.field_errors().contains_key("name"));
}
#[test]
fn byte_limit_preserves_multibyte_boundary() {
    assert!(validate_max_120_bytes(&"界".repeat(40)).is_ok());
    assert!(validate_max_120_bytes(&"界".repeat(41)).is_err());
}
```

- [ ] 给 CreateTeamInput/UpdateTeamInput 添加 `validator::Validate`，name使用custom非空；TeamMemberInput的agent_id使用同一个custom，成员向量标nested。它们既有 Serialize/Deserialize/JsonSchema derive保留。

```rust
#[validate(custom(function = "crate::backend::validation::validate_non_blank"))]
pub name: String,
#[validate(nested)]
pub members: Vec<TeamMemberInput>,
```

- [ ] 在 `create_team_sqlx/update_team_sqlx` 的任何DB写入之前调用 Validate，再保留 `validate_team_roster_members` 中唯一Leader、至少成员、成员id去重规则。删除原重复name空白和member.agent_id手写检查；trim存储行为保留。
- [ ] CatalogItem对 display_name/description/version/distributions 的通用规则改derive/custom；description继续用原 `MAX_TEXT_BYTES`；版本字符串非semver观测版本的语义不变；distribution ID重复、launch args NUL、cleanup占位规则仍在 `validate_basic`。
- [ ] 删除被derive接管的通用if，新增source guard按具体旧错误构造片段检测，避免把业务if清掉。把所有ValidationErrors转换集中到一个公开错误映射，不散落`.to_string()`。
- [ ] 测试字段错误不会把Agent环境值或任意输入放入WireError.details；输入校验失败时临时SQLite行数保持。

## 验证与边界

```bash
cargo test -p assetiweave --lib backend::validation::tests
cargo test -p assetiweave --lib team_name_field_uses_validator
cargo test -p assetiweave --lib backend::store::team_repo
cargo test -p assetiweave --lib backend::agent_market::types
cargo test -p assetiweave --lib backend::runtime::error
cargo fmt --all -- --check
```

成功：明确的字段校验代码被删、库实际在写入边界调用、空白/字节/业务约束均保真。停止：需要把路径存在性、跨记录唯一性、SQLite settings迁移或领域状态机塞进validator；这类逻辑保留原领域实现。本卡不承诺把全后端所有DTO一次迁完。

[官方 API：validator](https://docs.rs/validator/latest/validator/)
