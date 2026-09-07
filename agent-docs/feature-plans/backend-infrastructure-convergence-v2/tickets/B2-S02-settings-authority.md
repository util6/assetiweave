# B2-S02：Settings SQLite 单 Authority

**Authority:** 当前请求的 `AppRuntime` settings snapshot；持久 Authority 是同一 runtime 的 SQLite settings row。

**Contract:** C-SETTINGS-01、C-RUNTIME-02。

**Files:**

- Modify/Test: `src-tauri/src/backend/app_settings.rs`
- Modify/Test: `src-tauri/src/backend/application/session_memory.rs`
- Modify: `src-tauri/src/backend/application/system.rs`
- Modify only when needed for process entry installation: `src-tauri/src/lib.rs`

## Steps

- [ ] 用 `rg -n 'load_backend_settings_for_database|memory_.*_for_database|conversation_full_sync_on_startup_enabled_for_database' src-tauri/src` 列出全部 consumer。
- [ ] 增加 `session_memory_uses_its_own_runtime_settings_snapshot`：建立两个临时数据库，A 禁用 Memory、B 启用 Memory；不安装 process global，分别执行 phase1，断言 A cancel、B 执行。
- [ ] 增加 `sqlite_settings_ignore_corrupt_legacy_file_after_import` 的 AppService 行为测试：首次导入后破坏 legacy JSON，重新打开服务仍读取 SQLite 值。
- [ ] 运行新测试，记录 `_db` 被忽略或读取 process-global/legacy 文件造成的 RED。
- [ ] 删除同步 `load_backend_settings_for_database`；业务 workflow 从 `self.runtime.app_settings_value()` 的请求快照读取 typed `BackendSettings`，或显式 async 读取传入 pool。
- [ ] legacy JSON 只保留在 `load_or_import_app_settings_sqlx` 的“SQLite row 不存在”分支。
- [ ] `AppService::open_for_engine` 若仍是生产入口，必须安装或显式携带其 runtime；不得依赖另一个进程级 runtime。
- [ ] 删除查询必须证明 `_db: &Database` 被忽略的 helper 和正常业务 `read_settings_document` fallback 归零。

## Verify

```bash
cargo test -p assetiweave backend::app_settings -- --nocapture
cargo test -p assetiweave session_memory_uses_its_own_runtime_settings_snapshot -- --nocapture
cargo test -p assetiweave backend::runtime::tests -- --nocapture
cargo fmt --all -- --check
pnpm check:boundaries
```

提交：`fix(settings): 统一运行时设置读取权威`
