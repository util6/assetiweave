# A-F06：Query 接管应用设置读取与保存

> **Status: PLANNED**。执行时使用 `superpowers:executing-plans`，只执行本卡。

**Goal:** 移除设置Provider的自研持久化生命周期，SQLite设置仍是唯一持久权威。
**Architecture:** 应用级 settings Query + 串行 mutation；主题/字体应用是独立副作用；locale初始化由A-F10接管。
**Tech Stack:** TanStack Query、现有设置规范化函数，版本见依赖清单。
**Spec:** [Issue #22](https://github.com/util6/assetiweave/issues/22)。
**Depends:** A-F05、A-C01（Rust locale/设置契约已落地）。
**Contracts:** C-BASE、C-FRONTEND、C-SETTINGS。
**Read:** 入口、对应契约节、playbook。

## 文件与接口

- Modify: `frontend/src/store/settings/AppSettingsProvider.tsx`、`settingsPersistence.ts`、`frontend/src/app/AppProviders.tsx`、真实设置消费者。
- Create: `frontend/src/store/settings/settingsQueries.ts`、`useAppSettings.ts`、`SettingsEffects.tsx`、`settingsQueries.test.tsx`。
- Test: `AppSettingsProvider.test.ts`（实际测试schema）、`settingsPersistence.test.ts`、`components/settings/GlobalSettingsDialog.sync.test.tsx`、`theme/theme.test.ts`。
- Consumes: `getAppSettings():Promise<AppSettingsFile>`、`saveAppSettings(settings:AppSettings):Promise<AppSettingsFile>`、`normalizeStoredSettings(unknown):AppSettings`、`readCachedSettings/writeCachedSettings`（现有缓存方法，以源码签名为准）。
- Produces（本卡创建）:

```ts
export const appSettingsKey: readonly ["app-settings"];
export function settingsQueryOptions(): ReturnType<typeof queryOptions<AppSettingsFile>>;
export function useSaveAppSettings(): UseMutationResult<AppSettingsFile, unknown, AppSettings>;
export function SettingsEffects(): null;
// useAppSettings 在保留旧 updateSetting/resetSettings 的基础上新增：
// setColumnLayout(storageKey: string, weights: number[]): void;
```

options实际返回值保持推导；新增 mutationState selector 只读库缓存，例如 `useMutationState({filters:{mutationKey:["app-settings","save"]},select:(m)=>({id:m.mutationId,status:m.state.status,variables:m.state.variables})})`，不做 hook-local 顺序号。

options实际返回值保持推导，`AppSettingsFile.settings` 保持现有unknown，hook在select里规范化。`useAppSettings`从新文件导出，公开字段/更新方法与原provider hook保持同义，直接读取Query；不依赖新的Context。schema常量类型的消费者改直接从 `settingsSchema.ts` 导入。

## Red 与关键实现

先保存原schema与设置UI测试green。下列测试放 `settingsQueries.test.tsx`，验证backend响应覆盖启动cache；scope不含租户。

```ts
import { QueryClient } from "@tanstack/react-query";
import { expect, it, vi } from "vitest";
import { settingsQueryOptions, appSettingsKey } from "./settingsQueries";
const getAppSettings = vi.hoisted(() => vi.fn(async () => ({
  config_dir: "/tmp/app", config_path: "/tmp/app/data.db",
  conversation_adapter_dir: "/tmp/app/adapters", settings: { theme: "sunlight" },
})));
vi.mock("../../services/appSettings", () => ({ getAppSettings, saveAppSettings: vi.fn() }));

it("后端设置替换启动缓存，不按租户复制", async () => {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  client.setQueryData(appSettingsKey, { settings: { theme: "old-cache" } });
  const result = await client.fetchQuery({ ...settingsQueryOptions(), staleTime: 0 });
  expect(result.settings).toEqual({ theme: "sunlight" });
  expect(appSettingsKey).toEqual(["app-settings"]);
  client.clear();
});
```

跨 hook 实例的保存只使用同一个 `mutationKey: ["app-settings", "save"]` 和 `scope: {id:"app-settings"}`。Query cache 保存已确认服务响应，待保存/失败可重试的最新完整草稿从 Query MutationCache 选择，不在每个 hook 中建立私有顺序号。用 `useMutationState` 按 mutationKey 收集所有状态及 variables，按实际 Mutation 的 `mutationId` 排序（不只用可能同毫秒相同的 submittedAt）；取 mutationId 最大的一项：仅当其 status 为 pending/error 才显示 variables，否则显示已确认 Query cache。所有 useAppSettings 实例看到同一投影；最新成功时移除此前已 settled 的旧 mutation，防止成功记录 GC 后旧失败草稿复活。保留 pending 请求，不能删除队列中的操作。新 updateSetting 从该最新投影合成完整草稿，旧成功只更新已确认 cache，不覆盖较新草稿；旧错误只关联其 mutation，不把较新草稿回滚。

跨组件双实例测试必须覆盖：实例A改 theme、实例B紧接改 density；首保存失败/成功都不丢最新完整草稿，第二保存按 scope 顺序执行。仅 latest mutation 失败时显示该草稿和可重试动作；重试成功清除旧错误 mutation 的投影，否则旧失败不能永远覆盖数据库。Query MutationCache 就是协调状态，不再做一个并行 Map/Context Store。

`resetSettings` 提交 `{...defaultSettings, locale: current.locale}`；`setColumnLayout` 使用最新 settings 投影合并 columnLayouts 后经同一 mutation 保存。增加 `setColumnLayoutAsync(storageKey:string, weights:number[]): Promise<void>` 供 F13 一次导入等待后端成功再删旧 key；同步 void 方法是该异步方法的 UI 调用入口，错误仍可见。F13 不猜 mutation 成功时刻。

串行保存使用 `useMutation({ mutationFn: saveAppSettings, scope: { id: "app-settings" } })`。规范化及乐观更新遵守最新draft序号：旧save响应和旧save失败回滚均不得覆盖新draft；失败保持可见错误并允许重试。`initialData` 若来自cache须视为过期（`initialDataUpdatedAt:0`），不把默认值提交覆盖数据库。

## 步骤

- [ ] **Baseline**：保存设置默认值、CLI/Agent分配、同步偏好、theme测试结果。
- [ ] **Red**：加backend覆盖cache、新draft防旧响应/旧回滚、mount时不自动保存默认值、无Provider新hook可用的测试。
- [ ] **Migrate**：实际settings消费者切新hook；AppProviders装SettingsEffects而非设置Context；保留外观即时响应与缓存启动速度。
- [ ] **Clean**：删除原provider的fetch/save effects、lastPersistedSettingsRef及Context；删除生产代码对 `AppSettingsProvider.tsx` 的导入；schema和缓存纯函数继续独立保留。
- [ ] **Verify**：以下命令全部通过；AppSettingsFile服务签名不变化。

```sh
pnpm exec vitest run --config frontend/vite.config.ts frontend/src/store/settings frontend/src/components/settings/GlobalSettingsDialog.sync.test.tsx frontend/src/theme/theme.test.ts
pnpm typecheck
pnpm lint
```

## 验收与停止

SQLite数据读取、失败、串行保存、主题效果均有证据；本卡删除provider而非将旧provider套Query。A-C01定义的locale null必须原样保留，本卡不读取navigator填默认；初始化由A-F10处理。若后端设置作用域实际已改变，先核实C-SETTINGS再继续。

**API 来源:** [mutation scope](https://tanstack.com/query/latest/docs/framework/react/reference/useMutation)、[optimistic updates](https://tanstack.com/query/latest/docs/framework/react/guides/optimistic-updates)。
