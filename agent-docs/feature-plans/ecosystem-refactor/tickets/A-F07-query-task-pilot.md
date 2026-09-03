# A-F07：Query 接管搜索索引后台任务

> **Status: PLANNED**。执行时使用 `superpowers:executing-plans`，只执行本卡。

**Goal:** 以真实搜索索引任务验证事件写cache、单owner轮询、组件离开后仍跟踪。
**Architecture:** 查询/轮询归TanStack Query；Tauri订阅仅用窄事件桥连接cache；领域merge保留为纯函数。
**Tech Stack:** 已安装的TanStack Query，版本见依赖清单。
**Spec:** [Issue #22](https://github.com/util6/assetiweave/issues/22)。
**Depends:** A-F06。
**Contracts:** C-BASE、C-FRONTEND、C-TASK。
**Read:** 入口、契约对应节、playbook。

## 文件与接口

- Modify: `frontend/src/app/backgroundTasks/SearchIndexProvider.tsx`、`frontend/src/app/AppProviders.tsx`、`SearchIndexProvider.test.tsx`。
- Create: `frontend/src/app/query/taskKeys.ts`、`TaskEventBridge.tsx`、`TaskEventBridge.test.tsx`、`frontend/src/app/backgroundTasks/searchIndexQueries.ts`、`searchIndexQueries.test.tsx`。
- Consumes: A-F04 `QueryScope`；现有 `getConversationSearchIndexStatus`、`getConversationSearchIndexTask`、`startConversationSearchIndexRebuild`、`subscribeConversationSearchIndexTasks` 的签名与DTO。
- Produces（本卡创建）:

```ts
export const taskKeys: {
  resource(scope: QueryScope, domain: string): readonly ["tenant", string, number, "tasks", string];
};
export interface TaskEventBridgeProps<State, Event> {
  queryKey: QueryKey;
  subscribe(listener: (event: Event) => void): Promise<() => void>;
  merge?(current: State | undefined, event: Event): State | undefined;
}
export function TaskEventBridge<State, Event>(props: TaskEventBridgeProps<State, Event>): null;
export function mergeSearchIndexQueryState(previous: SearchIndexQueryState | undefined, incoming: SearchIndexQueryState): SearchIndexQueryState;
export interface SearchIndexQueryState {
  status: ConversationSearchIndexStatus | null;
  task: ConversationSearchIndexTaskSnapshot | null;
}
```

`searchIndexQueryOptions(scope)` 返回 `queryOptions<SearchIndexQueryState>` 的推导结果，key domain=`search-index`。现有 `useSearchIndex()` 返回字段、`rebuild():Promise<ConversationSearchIndexTaskSnapshot>`、`refresh():Promise<void>` 保持；其实现直接观察query。原Provider改为无Context的根运行组件，AppProviders将其与children并列放置；按现有文件名保留模块直至A-F08统一清理。

## Red 与关键实现

先运行SearchIndex现有测试green。下面放 `TaskEventBridge.test.tsx`，事件订阅晚完成仍必须释放，不需要业务fixture：

```tsx
/* @vitest-environment jsdom */
import { act, render } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { expect, it, vi } from "vitest";
import { TaskEventBridge } from "./TaskEventBridge";

it("卸载后到达的订阅句柄立即释放", async () => {
  const client = new QueryClient();
  const cleanup = vi.fn();
  let finish!: (cleanup: () => void) => void;
  const subscribe = vi.fn(() => new Promise<() => void>((resolve) => { finish = resolve; }));
  const view = render(<QueryClientProvider client={client}>
    <TaskEventBridge<number, number> queryKey={["task-probe"]} subscribe={subscribe} merge={(_, event) => event} />
  </QueryClientProvider>);
  view.unmount();
  await act(async () => { finish(cleanup); });
  expect(cleanup).toHaveBeenCalledTimes(1);
  client.clear();
});
```

桥只负责订阅、解除、订阅失败后1000ms重连；有 merge 时 `setQueryData`，没有 merge 时 `invalidateQueries({queryKey,exact:true},{cancelRefetch:false})`，供只发布通知的 Memory 事件使用。queryKey/subscribe/merge 在作用域内保持稳定引用，避免 render 重订阅；不拥有读取函数、loading、poll timer、task状态机。`useQuery({ ...options, refetchInterval: query => query.state.data?.task?.status === "running" ? 1000 : 10000, refetchIntervalInBackground:true })` 只在根任务组件启用。页面使用相同options但无interval、`enabled:false`，从cache观察；refresh显式调用QueryClient。

事件和查询响应共用 `mergeSearchIndexQueryState`：通过 Query `structuralSharing` 把 incoming 查询结果与当时 cache 合并，防止较早的 in-flight poll 在终态事件后覆盖缓存。同 ID 终态不回退 running；不同 ID 比较 started_at，保留较新的任务；null 不擦掉尚在执行的当前任务。status 用其 updated_at/source_revision 保留最新快照；无可靠新旧依据时使 root query 再读取权威状态，而不根据浏览器接收先后猜业务先后。测试至少复现 poll 已发出→收到终态事件→旧 running poll 回来，最终保持终态。

## 步骤

- [ ] **Baseline**：保存rebuild快速返回、terminal刷新状态、全局进度测试green。
- [ ] **Red**：新增桥释放测试；新增两页面observer仍一个poll owner、丢事件被poll补齐、terminal snapshot后高频轮询停止、低频10秒能发现漏掉启动事件的外部任务测试；fake timers结束时恢复real timers。
- [ ] **Migrate**：切真实SearchIndex到Query；start mutation立即把返回快照写cache；terminal事件使索引status重新读取；使用原merge防null快照抹掉运行中任务。
- [ ] **Clean**：SearchIndex不再调用 `useBackgroundTaskRuntime`，删除其Context/state/effect轮询；其他域暂仍使用旧runtime，A-F08逐域删除。
- [ ] **Verify**：以下命令通过；事件和poll同时到达不反转领域状态。

```sh
pnpm exec vitest run --config frontend/vite.config.ts frontend/src/app/query/TaskEventBridge.test.tsx frontend/src/app/backgroundTasks/searchIndexQueries.test.tsx frontend/src/app/backgroundTasks/SearchIndexProvider.test.tsx
pnpm typecheck
pnpm lint
```

## 验收与停止

桥是IPC适配而非第二Query框架；一个key恰好一个根轮询owner；切scope旧bridge晚事件忽略。Query observer自带各自计时器，只保证并发请求去重，不能把多个interval当成单一轮询。若任务是否tenant-scoped与C-TASK不一致，先报告证据再调整scope。

**API 来源:** [polling](https://tanstack.com/query/latest/docs/framework/react/guides/polling)、[QueryClient.setQueryData](https://tanstack.com/query/latest/docs/reference/QueryClient)。
