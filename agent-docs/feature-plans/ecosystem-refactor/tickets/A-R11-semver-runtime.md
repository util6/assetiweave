# A-R11：复用 semver 替代 Runtime 版本比较器

> **Status: PLANNED**。使用 `superpowers:executing-plans`。

**Goal:** 将通用数值比较交给已安装 semver，保留 runtime banner 和现有输入语法。
**Depends:** A-R04。
**Contracts:** C-BASE、C-ERROR、C-STORAGE。
**Gates:** G-RUST。

## 文件与接口

- Modify/Test: `src-tauri/src/backend/conversations/io_utils.rs`、`src-tauri/src/backend/conversations/tests.rs`。
- Read: `src-tauri/src/backend/conversations/package.rs`、`extension_kernel/identity.rs`；已有 semver 1/Cargo.lock，不新增依赖、不升级版本。
- 保持 `validate_runtime_version_constraint(&str)->AppResult<()>`、`runtime_version_satisfies_constraint(&str,&str)->AppResult<bool>`；内部 parse_minimum_version_constraint 返回 `semver::VersionReq`，detected parser 返回 `semver::Version`。
- Create internal helper: `parse_numeric_runtime_version(value:&str)->Option<semver::Version>`，只做既有 1–3 段数字到 Version::new 的归一化；不手工比较大小。

## 具体语义

输入 requirement 仍只接受 `>=x[.y[.z]]`；空格按原 trim，缺段补0，前导零按 u64 数字归一化，超 u64 拒绝。`^20`、`~20`、组合范围、第四段和 prerelease requirement 不因 semver 支持就放开。detected output 从第一段数字开始提取原数值串，保持 Python/Bash/Node banner 的旧规则；不把原来被忽略的 prerelease 后缀变成新门禁。

```rust
// minimum 已由 parse_numeric_runtime_version 归一化成 Version
let requirement = semver::VersionReq::parse(&format!(">={minimum}"))
    .map_err(AppError::external)?;
let accepted = requirement.matches(&detected);
```

```rust
#[test]
fn runtime_semver_preserves_minimum_language() {
    assert!(runtime_version_satisfies_constraint("v20.10.0", ">=20.2").unwrap());
    assert!(runtime_version_satisfies_constraint("Python 3.12.1", ">=3.12").unwrap());
    assert!(!runtime_version_satisfies_constraint("v18.19.0", ">=20").unwrap());
    assert!(runtime_version_satisfies_constraint("v20.0.0", ">=020").unwrap());
    assert!(validate_runtime_version_constraint("^20").is_err());
    assert!(validate_runtime_version_constraint(">=20.0.0.1").is_err());
}
```

测试放 `io_utils.rs` 内联 tests，以 `super::*` 引入当前私有函数；保留现有 conversations/tests 的调用断言。

## 步骤

- [ ] 先在旧实现运行上述 characterization，记录所有实际边界行为；新增采用 semver::VersionReq 的 source guard 才作为库接管 red。
- [ ] 用已有 regex 或简单 split 限定语法，返回 Version::new；VersionReq.matches 接管比较。保留当前 invalid-prefix 与 invalid-numeric 的公开错误分类，不把所有错误换码。
- [ ] 删除 io_utils 的 Vec 版本比较循环 `compare_versions`；只剩 banner 提取和严格语法归一化业务适配。
- [ ] 运行下列回归；核对没有新增 Agent core-range gate。

```sh
cargo test -p assetiweave --lib runtime_semver_preserves_minimum_language
cargo test -p assetiweave --lib runtime_version
cargo test -p assetiweave --lib conversations::tests
cargo test -p assetiweave --lib extension_kernel
cargo fmt --all -- --check
```

**范围界线：** `package.rs` 对历史包 min_core_version 的宽松解析属于独立包兼容面，与此处 runtime requirement 不是同一语言。本卡不更改其接受范围或宣称全仓所有版本逻辑已删；共享包兼容在任务二 W1 以明确 Manifest 决策收口。不要为增加 semver 调用数破坏已发布 Adapter 包。
**完成：** runtime 通用比较由 semver 真实执行，语法与 banner 兼容回归通过，无新 dependency。
**API:** [semver VersionReq](https://docs.rs/semver/latest/semver/struct.VersionReq.html)。
