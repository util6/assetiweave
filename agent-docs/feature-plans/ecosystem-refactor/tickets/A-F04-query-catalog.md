# A-F04：Query 接管租户与 Catalog 读取

> **Status: PLANNED**。执行时使用 `superpowers:executing-plans`，只执行本卡。

**Goal:** Catalog 实际页面从 Query cache 读取，不再由 hook 自建请求缓存。
**Architecture:** 一个 QueryClient；按租户与切换 epoch 分隔后端状态。租户作用域用一个窄 Context 提供身份，不复制 Catalog 数据。
**Tech Stack:** `../02-dependencies.md` 的 TanStack Query。
**Spec:** [Issue #22](https://github.com/util6/assetiweave/issues/22)。
**Depends:** A-F03。
**Contracts:** C-BASE、C-FRONTEND。
**Read:** 入口、契约对应节、playbook。

## 文件与接口

- Modify: `package.json`、`pnpm-lock.yaml`、`frontend/src/app/AppProviders.tsx`、`hooks/catalog/useCatalogData.ts`、`hooks/catalog/useCatalogController.ts`、`hooks/tenants/useTenantController.ts`、`services/catalog.ts`。
- Create: `frontend/src/app/query/queryClient.ts`、`QueryScopeProvider.tsx`、`catalogQueries.ts`、`catalogQueries.test.ts`、`QueryScopeProvider.test.tsx`。
- Test: `hooks/catalog/useCatalogData.test.ts`、`hooks/tenants/useTenantController.test.tsx`、`services/catalog.test.ts`。
- Consumes: `Tenant`、`AssetKind`、`Asset[]`、`Source[]`、`TargetProfile[]`、`AppOverview`、`AppShortcut[]`、`AssetMountStatus[]`、`NavigationModel`；其服务读函数保持签名。
- Produces（本卡创建）:

```ts
export interface QueryScope { tenantId: Tenant["id"]; epoch: number }
export function createAppQueryClient(): QueryClient;
export function QueryScopeProvider(props: { children: ReactNode }): ReactNode;
export function useQueryScope(): QueryScope | null;
```

`catalogKeys.root(scope)` 返回 `readonly ["tenant", string, number, "catalog"]`；其 `assets(scope, kind?)`、`sources(scope)`、`profiles(scope)`、`overview(scope)`、`shortcuts(scope)`、`mountStatuses(scope)`、`navigation(scope)` 在 root 后分别追加同名资源段，assets 再追加 `kind ?? "all"`。`assetsQueryOptions(scope, kind?)` 等同名 `*QueryOptions` 通过 `queryOptions` 推导准确服务返回类型；A-F05/A-F15 复用这些 keys，不另建同资源缓存。

`useTenantController` 保留现有 public 返回字段与方法；其 get/list 改 Query，其切换状态由 scope provider 提供。启动获取 active tenant 成功前，tenant-scoped queries disabled。切换时使旧 scope 不可写、取消旧 queries、调用服务、成功后递增 epoch 并发布新 scope；失败恢复旧租户但使用新 epoch 重读。IPC 本身不保证可中断，迟到结果只能落旧 key。AppSettings 是应用级资源，不按租户复制。

## Red 与关键实现

先运行现有 catalog/tenant 行为测试。新增下面完整测试到 `catalogQueries.test.ts`，证明 native cache 接管；初次因新模块缺失 red。

```ts
import { QueryClient } from "@tanstack/react-query";
import { afterEach, expect, it, vi } from "vitest";
import { assetsQueryOptions } from "./catalogQueries";
const listAssets = vi.hoisted(() => vi.fn(async () => []));
vi.mock("../../services/catalog", () => ({ listAssets }));
afterEach(() => vi.clearAllMocks());

it("同 scope 同 key 的并发 Catalog 读取只有一个请求", async () => {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const options = assetsQueryOptions({ tenantId: "workspace-a", epoch: 1 }, "skill");
  await Promise.all([client.fetchQuery(options), client.fetchQuery(options)]);
  expect(listAssets).toHaveBeenCalledTimes(1);
  expect(client.getQueryData(options.queryKey)).toEqual([]);
  client.clear();
});
```

`assetsQueryOptions` 使用 `networkMode: "always"`（本地 IPC）、明确 `staleTime`、`retry: false`（读错误先可见，暂不做业务错误自动重试），queryFn 调用 `listAssets(kind)`。QueryClient 默认值不把远程 HTTP 强行设 always。

给 `services/catalog.test.ts` 增加 desktop 错误例：`window.__TAURI_INTERNALS__` 存在、invoke reject 时 `listAssets` 必须 reject；preview 无 Tauri 时仍可 fallback。每个被本卡接管的读方法都检查，避免 Query 缓存假成功。

## 步骤

- [ ] **Baseline**：保存现有读取、导航延迟保存、租户切换测试结果。
- [ ] **Red**：新增去重测试、tenant A迟到结果不覆盖tenant B测试、desktop reject测试、`useCatalogData` 不含数据 `useState` 的接管guard。
- [ ] **Migrate**：安装依赖并在 AppProviders 最外层装一个 QueryClientProvider；真实 Catalog/租户消费者切到 options；`useCatalogData` 暂保留领域 facade，但数据唯一来自 cache。
- [ ] **Clean**：删除 Catalog 读取的 `loadCatalogData` 手工状态设置、重复 loading/cache；当前写入口转 `setQueryData`，写生命周期 A-F05 继续接管；不留镜像数组。
- [ ] **Verify**：运行下列命令；确认浏览器预览与desktop错误分别有断言。

```sh
pnpm exec vitest run --config frontend/vite.config.ts frontend/src/app/query/catalogQueries.test.ts frontend/src/app/query/QueryScopeProvider.test.tsx frontend/src/hooks/catalog/useCatalogData.test.ts frontend/src/hooks/tenants/useTenantController.test.tsx frontend/src/services/catalog.test.ts
pnpm typecheck
pnpm lint
```

## 验收与停止

真实页面依赖Query数据；快速切租户不串数据；同时两消费者只共享一份缓存。若现有 Engine 无法判断一次切换的完成状态，停止改scope并记录命令行为；不把tenant id仅写key而忽略服务实际全局租户切换。

**API 来源:** [query options](https://tanstack.com/query/latest/docs/framework/react/guides/query-options)、[network mode](https://tanstack.com/query/latest/docs/framework/react/guides/network-mode)、[cancellation](https://tanstack.com/query/latest/docs/framework/react/guides/query-cancellation)。
