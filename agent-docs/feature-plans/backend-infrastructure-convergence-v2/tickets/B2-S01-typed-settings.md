# B2-S01：建立原始 Settings 文档与后端类型化切片

**Objective:** 让后端业务读取类型字段，同时保留完整 JSON 与未知字段 round-trip。

**Contracts:** C-SETTINGS-01、C-ERROR-01、C-PRODUCT-01。

**Canonical authority after this card:** `AppSettingsDocument` 保留原始文档；`BackendSettings` 只投影后端拥有的字段。

**Interfaces:**

- Produce: `BackendSettings::from_document(&AppSettingsDocument) -> AppResult<BackendSettings>`
- Produce: `BackendSettings::merge_into_document(&self, document: AppSettingsDocument) -> AppResult<AppSettingsDocument>`
- Preserve: public settings payload remains complete JSON and schema version.

**Files:**

- Modify: `src-tauri/src/backend/app_settings.rs`
- Modify: `src-tauri/src/backend/store/settings_repo.rs`
- Modify as consumers require: `src-tauri/src/backend/ai_execution/composition.rs`
- Modify as consumers require: AppService modules that read `agentAssignments`, Memory exclusions, Conversation startup sync or AI runtime settings.
- Test: colocated settings/repository/Application tests.

## Steps

- [ ] Add RED tests for: unknown top-level/nested fields survive load-save-load; missing known fields get current defaults; wrong known field types return Validation; canonicalize twice is identical; v3/v4 migration is idempotent; locale first-writer behavior remains unchanged.
- [ ] Define typed slices for `memory`, `conversations`, `aiRuntime`, `agentAssignments`, `locale` and `columnLayouts`. Use camelCase serde names and current defaults from existing behavior.
- [ ] Keep the raw `serde_json::Value` in the document wrapper; typed parsing must not discard fields owned by newer frontend versions.
- [ ] Move normal business getters (`generationEnabled`, `usageEnabled`, exclusions, startup full sync, action assignment resolution) to typed fields.
- [ ] Keep `Value::get` only in migration and merge code; annotate each retained production use with the migration/unknown-field reason.
- [ ] Save by merging known slices into the raw document, then persist and return the actual stored document so locale concurrency semantics remain correct.
- [ ] Run all current Settings tests plus affected AI/Memory/Conversation resolver tests.

## Tests

```bash
cargo test -p assetiweave backend::app_settings -- --nocapture
cargo test -p assetiweave backend::store::settings_repo -- --nocapture
cargo test -p assetiweave backend::ai_execution -- --nocapture
cargo test -p assetiweave backend::application::tests -- --nocapture
```

## Delete proof

```bash
rg -n '\.get\(' src-tauri/src/backend/app_settings.rs
rg -n 'read_app_settings_value_for_database' src-tauri/src/backend --glob '*.rs'
```

通过：第一条只剩明确 migration/merge 位置；普通 business consumers 不再自行链式解析核心字段。

**Stop:** 发现前端专属展示字段时保留 raw JSON，不向 Rust typed slice 扩张；发现需要公开 payload 变化时停止并修订 Issue。

**Commit:** `refactor(settings): 引入后端类型化设置切片`
