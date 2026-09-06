# B2-F02：清除 Lossy 路径决策

**Authority:** OS 与文件系统逻辑使用 `Path`/`PathBuf`；UTF-8 持久化和 IPC 边界使用可失败转换。

**Contract:** C-PATH-01。

**Files:**

- Modify/Test: `src-tauri/src/backend/store/source_repo.rs`
- Modify/Test: `src-tauri/src/backend/application/skills.rs`
- Modify/Test: `src-tauri/src/backend/application/recent.rs`
- Modify/Test: `src-tauri/src/backend/target_catalog.rs`
- Modify/Test: `src-tauri/src/backend/defaults.rs`
- Modify only shared conversion helpers: `src-tauri/src/backend/host_paths.rs`, `src-tauri/src/backend/path_utils.rs`

## Steps

- [ ] 在 Unix 建立包含非 UTF-8 `OsString` 的 fixture，覆盖 Git root、Skill 导入匹配、Recent root 选择和 Target conflict key。
- [ ] 新测试命名为 `non_utf8_paths_never_alias_identity_or_persistence_keys`；断言文件系统比较保持无损，进入 UTF-8 持久边界时返回稳定 Validation/Storage error。
- [ ] 运行测试并记录当前 `to_string_lossy` 合并非法字节造成的 RED。
- [ ] `source_repo` 在 Git root 持久化前使用 `Utf8PathBuf::from_path_buf` 或现有等价可失败 helper。
- [ ] `skills` 使用 `Path` 比较，不把目标目录转换为 lossy `Cow<str>`。
- [ ] `recent` 使用路径组件排序；最终持久化使用可失败 UTF-8 转换。
- [ ] `target_catalog` 冲突键由无损规范化组件生成，非 UTF-8 输入显式报错。
- [ ] `defaults` 的默认 app-owned 路径使用可证明 UTF-8 的构造结果；转换失败不得静默回退到另一路径。
- [ ] 对全 backend 的剩余 `to_string_lossy` 输出分类：只有 UI display、已脱敏 diagnostic 和 test assertion 可以保留；把清单写入 Issue #24 交接，不建立长期 allowlist。

## Delete query

```bash
rg -n 'to_string_lossy' \
  src-tauri/src/backend/store/source_repo.rs \
  src-tauri/src/backend/application/skills.rs \
  src-tauri/src/backend/application/recent.rs \
  src-tauri/src/backend/target_catalog.rs \
  src-tauri/src/backend/defaults.rs
```

生产身份、比较、权限、冲突键和持久化路径中的命中必须为零；展示命中必须在交接中逐条说明。

## Verify

```bash
cargo test -p assetiweave non_utf8_paths_never_alias_identity_or_persistence_keys -- --nocapture
cargo test -p assetiweave backend::host_paths -- --nocapture
cargo test -p assetiweave backend::host_filesystem -- --nocapture
cargo test -p assetiweave backend::target_catalog -- --nocapture
cargo fmt --all -- --check
pnpm check:boundaries
```

提交：`refactor(paths): 清除业务路径的有损身份转换`
