# A-C01：SQLite 语言/分栏设置与原子语言导入契约

> **Status: PLANNED**。使用 `superpowers:executing-plans`；本卡只落后端/服务/设置类型，i18next 生产切换在 A-F10。

**Goal:** 在替换 i18n 前消除语言偏好的双权威与初始化竞争。
**Depends:** A00、A-R02、A-F04。
**Contracts:** C-BASE、C-SETTINGS、C-STORAGE。
**Gates:** G-FE、G-RUST、G-CONTRACT。

## 文件与接口

- Modify: `src-tauri/src/backend/app_settings.rs`、`src-tauri/src/backend/store/settings_repo.rs`、`src-tauri/src/backend/store/mod.rs`、`src-tauri/src/backend/application/params.rs`、`src-tauri/src/backend/application/system.rs`、`src-tauri/src/adapters/tauri/commands.rs`、`src-tauri/src/adapters/engine/registry.rs`。
- Modify: `frontend/src/store/settings/settingsSchema.ts`、`frontend/src/store/settings/AppSettingsProvider.test.ts`、`frontend/src/services/appSettings.ts`。
- Create: `frontend/src/services/appSettings.test.ts`（存在时扩展）。
- Generate: `cli/internal/schema/contract.json` 及 surface generator 实际产物。
- Test: Rust 上述模块内 `#[cfg(test)]`；前端 settings/service tests；Engine registry tests。

新增接口由本卡定义，不让后续模型猜名称：

```rust
// backend/app_settings.rs
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub(crate) enum AppLocale { Zh, En }
// AppLocale::as_str(self) -> &'static str，分别返回 "zh" / "en"。
// application/params.rs
#[derive(Debug, serde::Deserialize, schemars::JsonSchema)]
pub(crate) struct InitializeAppLocaleParams {
    pub(crate) locale: crate::backend::app_settings::AppLocale,
}
```

- AppService: `initialize_app_locale_if_unset(&self, locale: AppLocale) -> AppResult<AppSettingsFile>`。
- Store: `initialize_app_locale_sqlx(pool: &SqlitePool, locale: AppLocale) -> AppResult<Value>`；调用前确保 global row 已完成旧文档导入。
- Tauri: `initialize_app_locale_if_unset`，参数 `{locale}`。
- Engine registry: `settings.locale.initialize`，Write / App / 无额外高风险确认；schema 使用 InitializeAppLocaleParams，返回同一 AppSettingsFile。
- TS `settingsSchema.ts`: `export type AppLocale = "zh" | "en"`，新增 `locale: AppLocale | null`；default/旧 normalize 保持 null。另增 `columnLayouts: Record<string, number[]>`，default/旧 normalize 为 `{}`，供 A-F13 使用。

```ts
// services/appSettings.ts，沿用该文件现有 invoke 与 AppSettingsFile
export async function initializeAppLocaleIfUnset(
  locale: AppLocale,
): Promise<AppSettingsFile> {
  return invoke("initialize_app_locale_if_unset", { locale });
}
```

## 分栏偏好字段

`columnLayouts` 的 key 沿用现有 storageKey；value 是 2–16 项有限正数的权重数组。TS normalize 过滤非法 entry；Rust 对显式非法保存返回 Validation，旧数据缺失按空 map 规范化。客户端显示时另检查数组长度等于当前列数。保持未知的其他 settings 字段，不重建全量配置。测试空 map、合法 [1,2,1]、零/负数/超长数组，以及旧数据缺失；此字段与 locale 一起进入 schema 4。正常保存沿用 save_app_settings，不新增 layout IPC。旧 localStorage 尺寸的导入和提交时持久化由 A-F13 完成，本卡不改 ResizableColumns。

## 原子写入，不在前端先读后覆盖

初始化 SQL 核心（参数绑定；locale 来自 enum）：

```sql
UPDATE app_settings
SET settings_json = json_set(settings_json, '$.locale', ?1),
    updated_at = datetime('now')
WHERE settings_id = 'global'
  AND json_extract(settings_json, '$.locale') IS NULL;
```

然后读取当前 global row 返回；两次候选竞争仅第一位写入。`save_app_settings_sqlx` 的 upsert 在冲突更新时，对 locale 单独使用 `COALESCE(json_extract(excluded.settings_json, '$.locale'), json_extract(app_settings.settings_json, '$.locale'))` 写回新 JSON，避免老窗口提交 missing/null 覆盖合法值。这只是 locale 保护，不声称解决所有设置的跨窗口合并。

`SETTINGS_SCHEMA_VERSION` 3→4；读旧数据缺失 locale 规范为 null，保存显式非法值返回 Validation。所有旧文件导入/低版本 canonicalize 的写路径也必须遵守该 locale 原子保留规则。schema_version 高于当前仍报错；不修改历史 SQL migration，不引入新表。

## 核心回归测试骨架

在 settings_repo.rs tests 中定义测试，不依赖用户数据库：

```rust
#[tokio::test]
async fn locale_first_writer_wins_and_unrelated_save_preserves_it() {
    use crate::backend::app_settings::AppLocale;
    use serde_json::json;
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1).connect("sqlite::memory:").await.unwrap();
    sqlx::query("CREATE TABLE app_settings (settings_id TEXT PRIMARY KEY NOT NULL, schema_version INTEGER NOT NULL, settings_json TEXT NOT NULL, updated_at TEXT NOT NULL)")
        .execute(&pool).await.unwrap();
    super::save_app_settings_sqlx(&pool, 4, &json!({"theme":"sunlight"}))
        .await.unwrap();
    super::initialize_app_locale_sqlx(&pool, AppLocale::En).await.unwrap();
    let second = super::initialize_app_locale_sqlx(&pool, AppLocale::Zh).await.unwrap();
    assert_eq!(second["locale"], "en");
    super::save_app_settings_sqlx(&pool, 4, &json!({"theme":"sunlight", "locale":null}))
        .await.unwrap();
    let (_, stored) = super::load_app_settings_sqlx(&pool).await.unwrap().unwrap();
    assert_eq!(stored["locale"], "en");
    assert_eq!(stored["theme"], "sunlight");
}
```

再用同 pool 的 `tokio::join!` 同时初始化 zh/en，断言最终为其中一个，之后反向候选不改变它；显式 save zh 可改变 en。AppService 层覆盖 v3→v4、未知字段保留、非法值 rejection；TS normalize 缺失/null→null、zh/en→原值，service mock invoke 的命令与 payload 严格相等。

## 步骤

- [ ] 跑 settings 原测试建立 green，加入新测试证明旧实现缺字段/竞争保护。
- [ ] 加 enum/DTO、canonicalize 与原子 SQL；所有写入路径使用统一 locale 保护。
- [ ] 接入 AppService、Tauri invoke_handler、Engine registry，更新生成契约和 surface；前端仅添加类型/服务，不提前引入 i18next。
- [ ] 运行下列命令，核查真实执行的测试数量和生成 diff。

```sh
cargo test -p assetiweave app_settings
cargo test -p assetiweave settings_repo
cargo test -p assetiweave adapters::engine
pnpm exec vitest run --config frontend/vite.config.ts frontend/src/store/settings frontend/src/services/appSettings.test.ts
pnpm typecheck
pnpm cli:contract
pnpm gen:surface-matrix
pnpm check:surface-matrix
go test -C cli ./internal/...
```

- [ ] 交接 A-F06/F10：locale 仍可为 null，首次 UI bootstrap 由 F10 调用；reset 保留 locale 在 F06 实现。新增命令是附加能力，不更改现有 wire envelope/protocol version；若执行时出现实际破坏性字段变更，走 A-C02 的版本审查，不能静默变更。

**完成条件：** 新命令真实可经 Tauri/Engine 到同一 store；并发导入/旧保存不会覆盖已初始化语言，旧数据保留。单元测试 SQL 的通过不能代替上层注册与生成物检查。
