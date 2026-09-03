# A-F03：TanStack Router 接管实际工作区导航

> **Status: PLANNED**。执行时使用 `superpowers:executing-plans`，一轮只做本卡。

**Goal:** 保持导航体验，由成熟 Router 管理匹配、历史、懒加载和 pending/error 状态。
**Architecture:** memory history + code-based route tree；应用布局仍是业务组件，持久化菜单模型仍来自 Rust。不存在新旧 Router 双路运行。
**Tech Stack:** `../02-dependencies.md` 锁定的 `@tanstack/react-router`。
**Spec:** [Issue #22](https://github.com/util6/assetiweave/issues/22)。
**Depends:** A-F02。
**Contracts:** C-BASE、C-FRONTEND、C-UI。
**Read:** 入口、契约相关节、playbook；触及 Memory 导航时额外读 `../../memory-rewrite/00-execution-router.md`。

## 文件与接口

- Modify: `package.json`、`pnpm-lock.yaml`、`frontend/src/router/AppRouter.tsx`、`routes.ts`、`navigationTargets.ts`、`frontend/src/layouts/app/AppLayout.tsx`、真实导航调用方。
- Create: `frontend/src/router/routeTree.tsx`、`navigationPath.ts`、`createAppRouter.ts`、`createAppRouter.test.ts`、`navigationPath.test.ts`。
- Test: 既有 `AppRouter.test.tsx`、`routes.test.ts`、`RouteTransition.test.tsx` 的业务断言迁入新实现测试。
- Consumes: 现有 `NavigationModel`、`ConversationNavigationTarget`、`MemoryNavigationTarget` 和页面 props；本卡不修改 Rust 菜单 DTO。
- Produces（本卡创建）:

```ts
// navigationPath.ts；输出是应用内部路径，不写浏览器 URL。
export function navigationPath(model: NavigationModel, activeSubNavId: string): string;
// createAppRouter.ts；AppRouterInstance = ReturnType<typeof createAppRouter>
export function createAppRouter(initialPath?: string): ReturnType<typeof createRouter>;
```

实现时让函数返回值由实际 `createRouter({ routeTree, history })` 推导，避免宽化丢失类型安全；上面签名描述职责，不要求显式写宽返回注解。

路径表：`skills.overview → /skills/overview`、`skills.sources → /skills/sources`、`skills.groups → /skills/groups`、`skills.mounts → /skills/mounts`、`conversations.sessions → /conversations/sessions`、`conversations.web-records → /conversations/web-records`、`prompts.overview → /prompts/overview`、`memory.recent|recall → 对应 /memory/...`、`team.overview → /team/overview`；其余进入 `/under-construction`。已退休入口继续经过 `normalizeNavigationModelRoutes` 规范化。

## Red 与关键实现

先保留路由行为 characterization green；新 Router 接管测试 red，测试不渲染页面所以不依赖页面 mock。

```ts
/* @vitest-environment jsdom */
import { expect, it } from "vitest";
import { createAppRouter } from "./createAppRouter";

it("工作区导航只改 memory history", async () => {
  const original = window.location.href;
  const router = createAppRouter("/skills/overview");
  await router.navigate({ to: "/skills/groups" });
  expect(router.state.location.pathname).toBe("/skills/groups");
  expect(window.location.href).toBe(original);
});
```

官方关键 API：

```ts
import { createMemoryHistory, createRouter } from "@tanstack/react-router";
import { routeTree } from "./routeTree";
export function createAppRouter(initialPath = "/skills/overview") {
  return createRouter({ routeTree, history: createMemoryHistory({ initialEntries: [initialPath] }) });
}
```

routeTree 使用 `createRootRoute/createRoute`、`Outlet`，具体页用 `lazyRouteComponent`；菜单排序与可见性仍取 NavigationModel。原 AppRouter 中通知、备份完成处理先移到明确业务 hook，在 A-F05/A-F07 接管，不塞进 route loader。Conversation/Memory 定位目标由类型化上下文或现有事件调用传递，不把大对象塞入 path。

## 步骤

- [ ] **Baseline**：运行既有 Router 三个测试并保留入口、退休菜单、定位行为断言。
- [ ] **Red**：加入 memory-history 测试与 source guard：AppRouter 引用 `RouterProvider`；不再引用 `resolveAppRoute`/`preloadRoute`。
- [ ] **Migrate**：创建路由树、切换真实 AppRouter；用 pending/error component 接管加载状态；导航保存继续现有120ms合并直到 A-F05迁移。
- [ ] **Clean**：删除 `routeLoaders.ts` 的 `createCachedLoader/routeRegistry/preloadRoute` 与 `routes.ts` 的匹配引擎；保留菜单规范化业务函数。删除 RouteTransition 自制 timer 生命周期，视觉 skeleton 组件可保留。
- [ ] **Verify**：新旧业务断言在新 Router 上通过，真实导航只走一棵树。

```sh
pnpm exec vitest run --config frontend/vite.config.ts frontend/src/router
pnpm typecheck
pnpm lint
pnpm build
```

## 验收与停止

所有当前可达页面可通过新树进入；不新增浏览器 URL/deep-link 产品行为；旧路由实现零生产引用。若存在测试未覆盖的隐藏导航入口，先补 characterization 再接管；不要静默删除入口。性能 skeleton 行为与 ADR0010 冲突时报告实际复现，不恢复第二套路由器。

**API 来源:** [history types](https://tanstack.com/router/latest/docs/guide/history-types)、[code-based routing](https://tanstack.com/router/latest/docs/routing/code-based-routing)。
