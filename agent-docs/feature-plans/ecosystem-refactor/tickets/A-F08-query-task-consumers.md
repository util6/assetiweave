# A-F08：剩余后台任务迁移并删除自研请求运行时

> **Status: PLANNED**。执行时使用 `superpowers:executing-plans`；本卡内部逐域小步验证，禁止一次改完才跑测试。

**Goal:** 任务快照与通用查询生命周期归Query，Memory/Team的业务投影保持正确。
**Architecture:** 根级后台任务query组件并列装配；TeamSession按team范围装配；事件桥复用A-F07，后台DTO仍采用当前契约。
**Tech Stack:** 已安装TanStack Query，版本见依赖清单。
**Spec:** [Issue #22](https://github.com/util6/assetiweave/issues/22)。
**Depends:** A-F07。
**Contracts:** C-BASE、C-FRONTEND、C-TASK。
**Read:** 入口、契约相关节、playbook；本卡触及Memory，另读 `../../memory-rewrite/00-execution-router.md`，只保留其产品语义，迁移执行范围仍以本卡为准。

## 文件与接口

- Modify（穷尽本卡provider范围）: `frontend/src/app/backgroundTasks/{ConversationSyncProvider,ConversationDataMaintenanceProvider,AiExecutionTaskProvider,AgentLifecycleTaskProvider,MemoryTaskProvider,SkillBackupProvider,CatalogTaskProvider,TeamTaskProvider,TeamSessionProvider}.tsx`；`frontend/src/app/AppProviders.tsx`；这些hook的真实import消费者。
- Create: `frontend/src/app/backgroundTasks/BackgroundTaskQueries.tsx`、`taskQueryConsumers.test.tsx`、`TeamTaskProvider.test.tsx`（原仓库没有该测试）。
- Test: 对应现有Provider测试、`TeamSessionStore.test.ts`、`AppClosePrompt.test.tsx`、`pages/conversations/ConversationsPage.sync.test.tsx`、`pages/memory/MemoryPage.test.tsx`、`pages/team/TeamPage.test.tsx`。
- Consumes: A-F07 `TaskEventBridge`、`taskKeys.resource(scope,domain)`；A-F04 `QueryScope`；各service现有快照与start/cancel/retry方法，不增加统一任务DTO。
- Produces: `BackgroundTaskQueries():ReactNode` 只挂载根任务observer/桥；现有领域hook保留同义返回字段与命令签名，但读取Query。各provider文件导出其无children根运行组件，原Context删除；消费者可在本卡原位更名，更新所有import后删除旧文件。

资源domain固定为 `conversation-sync`、`conversation-maintenance`、`ai-execution`、`agent-lifecycle`、`memory`、`skill-backup`、`catalog-source-scan`、`catalog-batch-mount`、`catalog-skill-acquire`、`team-run`。TeamSession key在 `taskKeys.resource(scope,"team-session")` 后追加teamId，保持一个team root owner；不把每个MemberView各开poll。

领域保留清单：Memory `MemoryTaskView`过滤与cancel/retry；TeamSession `TeamSessionStoreState`、MAX_REPLAY_CONCURRENCY=2、乱序merge、256items/32executions上限、未读；Catalog批量冲突禁用、终态一次刷新；各任务错误信息与退出提示。服务事件是否携带完整snapshot不同，Memory可以用事件invalidate而非猜字段。

## Red 与关键实现

逐域先运行已有behavior测试green，再加入新库接管guard。下面放 `taskQueryConsumers.test.tsx`；旧字符串由数组拼接，避免guard自身被后续扫描误计。

```ts
import { readFileSync } from "node:fs";
import { expect, it } from "vitest";
const names = ["ConversationSync", "ConversationDataMaintenance", "AiExecutionTask", "AgentLifecycleTask", "MemoryTask", "SkillBackup", "CatalogTask", "TeamTask", "TeamSession"];
it.each(names)("%s 不再拥有自研请求运行时或poll interval", (name) => {
  const source = readFileSync(new URL(`./${name}Provider.tsx`, import.meta.url), "utf8");
  expect(source).not.toContain(["useBackground", "TaskRuntime"].join(""));
  expect(source).not.toContain("setInterval(");
});
```

如果本卡更名文件，测试同步改为实际新路径列表；不是删除该域检查。另在现有 `TeamSessionProvider.test.tsx` 用已定义fixture增加“双订阅UI＋单query owner”调用次数断言；保留其乱序/replay测试原数据，不用cast伪造不完整DTO。

## 步骤

- [ ] **Baseline**：按文件列表运行现有测试；记下每域读/start/event/terminal行为。
- [ ] **Red**：先启用当前要迁移域guard，记录旧runtime引用red；添加该域事件丢失/快照晚到回归。每域完成后启用下一域guard。
- [ ] **Migrate**：依序Memory→SkillBackup→ConversationSync→Maintenance→AI→AgentLifecycle→Catalog→TeamRun→TeamSession；每域根query观察、页面cache观察、start seed、event bridge一次接管。
- [ ] **Clean**：所有真实调用方不再导入后，删除 `BackgroundTaskRuntime.tsx` 与只测其旧实现的测试；领域merge测试移到仍保留的纯函数旁；AppProviders不再九层Context嵌套。
- [ ] **Verify**：每域跑单文件；最终跑下面集合与close prompt。提供源文件diff范围、移除符号清单。

```sh
pnpm exec vitest run --config frontend/vite.config.ts frontend/src/app/backgroundTasks frontend/src/app/query/TaskEventBridge.test.tsx frontend/src/app/AppClosePrompt.test.tsx frontend/src/pages/conversations/ConversationsPage.sync.test.tsx frontend/src/pages/memory/MemoryPage.test.tsx frontend/src/pages/team/TeamPage.test.tsx
pnpm typecheck
pnpm lint
```

## 验收与停止

生产旧runtime零引用；非冲突UI可操作；scope切换旧事件不污染新cache；任务状态来源仍后端。若一个域的新旧行为差异超过请求机制（例如要重写Team编排），停止该域并报告；本卡不启动插件架构任务，也不把Team投影塞进Zustand。

**API 来源:** [polling](https://tanstack.com/query/latest/docs/framework/react/guides/polling)、[dependent queries](https://tanstack.com/query/latest/docs/framework/react/guides/dependent-queries)。
