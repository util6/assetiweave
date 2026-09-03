# A-F05：Query 接管 Catalog 写入和失效

> **Status: PLANNED**。执行时使用 `superpowers:executing-plans`，只执行本卡。

**Goal:** 保存、挂载与批量完成通过 mutation/cache 更新，删除页面级重复刷新链。
**Architecture:** 使用库 mutation；保留业务命令与结果解释，失效范围由有限资源列表表达。
**Tech Stack:** A-F04 已安装的 TanStack Query，版本见依赖清单。
**Spec:** [Issue #22](https://github.com/util6/assetiweave/issues/22)。
**Depends:** A-F04。
**Contracts:** C-BASE、C-FRONTEND、C-TASK。
**Read:** 入口、契约对应节、playbook。

## 文件与接口

- Modify: `frontend/src/hooks/catalog/useCatalogData.ts`、`useCatalogController.ts`、`useCatalogOperations.ts`、`useMountSelection.ts`、`frontend/src/router/AppRouter.tsx`、`frontend/src/services/catalog.ts`。
- Create: `frontend/src/app/query/catalogMutations.ts`、`catalogMutations.test.ts`、`navigationMutation.test.tsx`。
- Test: 既有 catalog controller/data、sources controller 测试。
- Consumes: A-F04 `QueryScope`、`catalogKeys`、`createAppQueryClient`；现有 `updateAssetDescription(assetId, description):Promise<Asset>`、`mountAssetMount/unmountAssetMount:Promise<AssetMountUpdateResult>`、`updateNavigationModel(model):Promise<NavigationModel>`、`updateAppShortcuts(shortcuts):Promise<AppShortcut[]>`。
- Produces（本卡创建）:

```ts
export type CatalogInvalidation = "assets" | "sources" | "profiles" | "overview" | "mountStatuses" | "shortcuts";
export function invalidateCatalog(client: QueryClient, scope: QueryScope, resources: readonly CatalogInvalidation[]): Promise<void>;
export function useUpdateAssetDescription(scope: QueryScope): UseMutationResult<Asset, Error, { assetId: string; description: string | null }>;
export function useSaveNavigation(scope: QueryScope): {
  save(model: NavigationModel): Promise<NavigationModel>;
  schedule(model: NavigationModel): void;
};
```

`useSaveNavigation.schedule` 保持120ms合并与立即更新UI，非通用debounce框架；串行 mutation 使用 `scope: { id: JSON.stringify(catalogKeys.navigation(scope)) }`。完成写回之前检查最新本地导航版本，旧响应不回退新选择。卸载清除尚未提交timer。其余写hook返回库 `UseMutationResult`，不创建通用CRUD工厂。

## Red 与关键实现

保持既有导航120ms测试green。新增失效去重测试到 `catalogMutations.test.ts`：

```ts
import { QueryClient } from "@tanstack/react-query";
import { expect, it, vi } from "vitest";
import { invalidateCatalog } from "./catalogMutations";
import { catalogKeys } from "./catalogQueries";

it("批量完成每个资源只失效一次", async () => {
  const client = new QueryClient();
  const invalidate = vi.spyOn(client, "invalidateQueries").mockResolvedValue();
  const scope = { tenantId: "workspace-a", epoch: 1 };
  await invalidateCatalog(client, scope, ["mountStatuses", "overview", "mountStatuses"]);
  expect(invalidate).toHaveBeenCalledTimes(2);
  expect(invalidate).toHaveBeenCalledWith({ queryKey: catalogKeys.mountStatuses(scope) });
  client.clear();
});
```

此测试首先因函数缺失red；另给真实controller加任务完成事件重复到达仍只刷新一次的行为测试。失效helper内部直接 `new Set(resources)` + `client.invalidateQueries`，没有新缓存。

## 步骤

- [ ] **Baseline**：执行既有 Catalog 测试，保存批量选择、部分失败、exclusive mount 通知断言。
- [ ] **Red**：加入失效去重、写失败不伪造成功、旧导航save晚到不覆盖新导航、旧tenant mutation不写当前cache的测试。
- [ ] **Migrate**：真实写消费者使用 `useMutation`；返回Asset/Status用 `setQueryData` 精确更新；需要后端重算的overview用失效，后台任务start仅接收快照不等待完整作业。
- [ ] **Clean**：删除对应 `refreshOverview/refreshProfiles/refreshCatalogAndMountState` 重复Promise.all；领域facade若保留名称，内部仅调用keys失效。合并controller与Router重复的任务完成刷新责任，每一类完成副作用只有一个owner。
- [ ] **Verify**：执行下面命令；本卡不改资产挂载数据库规则。

```sh
pnpm exec vitest run --config frontend/vite.config.ts frontend/src/app/query/catalogMutations.test.ts frontend/src/app/query/navigationMutation.test.tsx frontend/src/hooks/catalog/useCatalogData.test.ts frontend/src/hooks/catalog/useCatalogController.test.tsx frontend/src/hooks/sources/useSourcesController.test.tsx
pnpm typecheck
pnpm lint
```

## 验收与停止

请求状态由Mutation拥有；一个业务结果只触发一次必要刷新，失败路径保留可见错误。若后台事件缺少稳定task identity而无法去重，报告协议事实并停止该类迁移，不猜测timestamp充当版本。原始同步CLI/Engine业务不由本卡改写。

**API 来源:** [mutations](https://tanstack.com/query/latest/docs/framework/react/guides/mutations)、[invalidation](https://tanstack.com/query/latest/docs/framework/react/guides/query-invalidation)。
