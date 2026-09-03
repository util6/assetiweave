# A-F15：迁移剩余共享缓存消费者并删除 asyncCache

> **Status: PLANNED**。使用 `superpowers:executing-plans`，一轮只做本卡；按四个消费者逐个验证。

**Goal:** 移除 `lib/asyncCache.ts` 的第二套请求缓存，所有真实消费者复用 Query。
**Depends:** A-F14、A-F05、A-F08。
**Contracts:** C-BASE、C-FRONTEND、C-TASK。
**Gates:** G-FE、G-BEHAVIOR。

## 文件与接口

- Modify: `frontend/src/hooks/sources/useSourcesController.ts` 及其测试、`frontend/src/pages/groups/SkillGroupsPage.tsx`、`frontend/src/pages/mounts/SkillMountsPage.tsx`、`frontend/src/pages/conversations/ConversationsPage.tsx`、`ConversationsPage.sync.test.tsx`（同目录）。
- Modify/Create: `frontend/src/app/query/catalogQueries.ts`、`conversationQueries.ts`、`conversationQueries.test.ts`。
- Delete: `frontend/src/lib/asyncCache.ts` 以及只测试旧机制的对应测试（若存在）；所有 `clearSharedResourceCache` 测试 setup 改为独立 QueryClient 清理。
- Consumes: A-F04 QueryScope、catalogKeys/options；现有 `listSkillSources/listSkillAssets/listSkillGroups` 与 Conversation services，签名保持。
- Produces: Catalog 增 `groups(scope)` key，与 Groups/Mounts 两页面共享；如 skill sources/assets 服务查询语义与通用列表不同，使用 root 下明确 `skillSources/skillAssets` 段，不误用同 key 缓存不同返回值。Conversation keys 包含 scope、recordKind，以及 sessions 的 search/filter/pagination 参数。

## 行为与测试

创建 `conversationKeys` 对象：`root(scope)` 为 `['tenant',scope.tenantId,scope.epoch,'conversations']`；`adapters(scope,recordKind)` 和 `sessions(scope,recordKind,search)` 在 root 后追加各自资源与输入。现有分页/过滤进入 queryFn 时必须同步进入 key，不用 ref 隐藏 query 输入。

```ts
import { expect, it } from "vitest";
import { conversationKeys } from "./conversationQueries";
it("会话缓存按租户、记录类型与搜索隔离", () => {
  const a = {tenantId:"tenant-a",epoch:1};
  const b = {tenantId:"tenant-b",epoch:1};
  expect(conversationKeys.sessions(a,"session","alpha"))
    .not.toEqual(conversationKeys.sessions(b,"session","alpha"));
  expect(conversationKeys.sessions(a,"session","alpha"))
    .not.toEqual(conversationKeys.sessions(a,"session","beta"));
});
```

recordKind 类型从 `frontend/src/types/index.ts` 导入 `ConversationRecordKind = "session" | "web"`；不扩大枚举。

## 步骤

- [ ] `rg -n 'SharedResource|lib/asyncCache' frontend/src` 列全部生产/测试调用方；若四类之外有新增消费者，先更新本卡闭包，不提前删文件。
- [ ] Sources：sources/assets/loading/error 改 useQuery；refetch 复用失效/刷新，选中项/导入对话框状态保持局部；跑 useSourcesController tests。
- [ ] Groups + Mounts：统一 listSkillGroups options，两页面共享请求；列表写操作复用 A-F05 mutation invalidation，未完成挂载任务不能提前标成功。加两 observer 去重与批次完成单次刷新断言。
- [ ] Conversations：adapters/sessions 改 Query，保持 recordKind/search 与分页/selection、同步任务 UI；失效只刷新当前有效作用域，旧搜索迟到结果不覆盖新搜索。扩展 sync.test.tsx 的事件/polling收敛与会话定位断言。
- [ ] 删除各消费者用于后端列表的 useState/手工 promise/loading lifecycle，保留用户输入和领域投影。所有消费者切完删除 asyncCache 与清理测试缓存的全局函数。
- [ ] 执行命令，确认 grep 无生产残留但不将 grep 当行为证据。

```sh
pnpm exec vitest run --config frontend/vite.config.ts frontend/src/hooks/sources/useSourcesController.test.tsx frontend/src/pages/conversations/ConversationsPage.sync.test.tsx frontend/src/app/query
pnpm typecheck
pnpm test
pnpm lint
rg -n 'SharedResource|lib/asyncCache' frontend/src
```

**完成：** 四类真实调用方只有 Query 缓存；同 scope 合并与跨 scope 隔离均有运行证据；第二套 Map/promise/cache 管理彻底删除。
