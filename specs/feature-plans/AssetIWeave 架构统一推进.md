

> **AssetIWeave 前端现在处于一个明显的“架构迁移中间态”**：很多新的、更好的抽象已经出现了，但旧实现没有完全迁过去，于是形成了若干“双轨制”。

它不是“整体架构很差”，相反，我能看出你是在逐渐引入 `foundation`、controller、route loader、background task provider、theme recipe 等新思路。真正的问题是：**这些新思路很多只完成了 50%～80%，缺少最后一次收口。**

我目前认为比较明显的有下面 7 个。

---

# 1. P0：Modal 是最典型的“双轨制”

这个我们刚讨论过：

```text
ConfirmDialog / LogViewer
        ↓
DialogFrame
        ↓
Radix Dialog

GlobalSettingsDialog
        ↓
手写 fixed / aria-modal / scroll lock
```

这是最标准的：

> 新基础设施已经建立，但老的特殊页面没有迁入。

所以你最近碰到的 scroll lock、z-index、nested dialog 都是这种“双轨”产生的后果。

这个应该优先收口。

---

# 2. P0/P1：实际上存在两套 Design System 组件层

这个比 Dialog 还直观。

你现在同时有：

```text
components/ui/
    button.tsx
    card.tsx
    input.tsx
    switch.tsx

components/foundation/
    Badge
    DialogFrame
    EmptyState
    FieldFrame
    PageHeader
    Panel
    SurfaceButton
    ...
```

`ui + foundation` 两层本身没有问题。

真正的问题是，边界并没有完全建立起来。

最典型的是：

```text
ui/Button
foundation/SurfaceButton
```

两份实现几乎是同一个组件。

`Button`：

```tsx
const Comp = asChild ? Slot : "button";

return (
  <Comp
    className={cn(surfaceButtonRecipe({ className, size, variant }))}
    ref={ref}
    {...props}
  />
);
```

`SurfaceButton`：

```tsx
const Comp = asChild ? Slot : "button";

return (
  <Comp
    className={cn(surfaceButtonRecipe({ className, size, variant }))}
    ref={ref}
    {...props}
  />
);
```

这已经不是“类似组件”，而是**两个名字不同的同级 Primitive**。

这很像一次组件体系重构留下来的迁移残余。

### 我建议明确三层

```text
theme/
    ↓
Design Tokens / Recipes

components/ui/
    ↓
Primitive
Button
Input
Switch
Card
Dialog Primitive wrapper

components/foundation/
    ↓
App-level Composite
PageHeader
FieldFrame
DialogFrame
EmptyState
FullscreenDialogFrame

features / pages
    ↓
业务组件
```

那么：

```text
SurfaceButton
```

就不应该再自己实现一次。

要么删除：

```text
SurfaceButton → Button
```

要么如果你真的特别喜欢这个语义名字：

```ts
export { Button as SurfaceButton } from "../ui/button";
```

但不能维护两份实现。

### 这和刚才的 Radix 问题属于同一种病

都是：

> **新的 Foundation/Primitive 思路已经有了，但是代码没有完全迁移。**

---

# 3. P1：Router 也正在“双轨运行”

这一点非常明显。

你现在已经建立了一个不错的新基础设施：

```text
routeLoaders.ts
```

里面有：

```ts
loadCatalogPage
loadConversationsPage
loadSkillGroupsPage
loadPromptOverviewPage
loadSkillMountsPage
loadSourcesPage
```

还有统一的：

```ts
createCachedLoader()
preloadRoute()
```

这是很好的方向：

```text
Route
↓
Route Registry
↓
Loader
↓
Preload
↓
Page
```

但是 `AppRouter` 只迁了一部分。

一部分：

```tsx
const CatalogPage = lazy(loadCatalogPage);
const ConversationsPage = lazy(loadConversationsPage);
const SkillGroupsPage = lazy(loadSkillGroupsPage);
```

另一部分还是：

```tsx
const LogViewerModal = lazy(() => import(...));
const ManualPage = lazy(() => import(...));
const MemoryPage = lazy(() => import(...));
```

更重要的是，最终 page resolution 仍然是一大片：

```tsx
routeId === "conversations"
  ? ...
  : routeId === "skill-mounts"
    ? ...
    : routeId === "skill-groups"
      ? ...
      : routeId === "prompts-overview"
        ? ...
        : routeId === "sources"
          ? ...
```

所以现在 Router 实际有两套 authority：

```text
routes / routeLoaders
        +
AppRouter imperative switch
```

我不会说你必须换 React Router。

**Tauri Desktop App 自己做 Router 完全没问题。**

问题不是“没用 React Router”，而是：

> 你自己的新 Router abstraction 已经建立起来了，但 `AppRouter` 还没有完全成为它的 Consumer。

最终应该向：

```ts
const routeRegistry = {
  conversations: {
    loader: loadConversationsPage,
    skeleton: "conversations",
    render: ...
  },

  memory: {
    loader: loadMemoryPage,
    skeleton: "memory",
    render: ...
  },
};
```

靠近。

让：

```text
AppRouter
```

更多只负责：

```text
resolve route
→ render route
→ transition
```

而不是知道每一个页面怎么加载、需要哪些特殊分支。

---

# 4. P1：Background Task 已经抽象了，但“抽象少了一层”

这个是我认为最值得下一阶段处理的架构点之一。

你现在有：

```text
ConversationSyncProvider
AiExecutionTaskProvider
AgentLifecycleTaskProvider
MemoryTaskProvider
SearchIndexProvider
SkillBackupProvider
```

全部挂在全局：

```tsx
<ConversationSyncProvider>
  <AiExecutionTaskProvider>
    <AgentLifecycleTaskProvider>
      <MemoryTaskProvider>
        <SearchIndexProvider>
          <SkillBackupProvider>
```

这说明你已经意识到了：

> Background Task 是 application-level state，而不是 Page local state。

这个方向是正确的。

但是你现在变成了：

```text
每一种 Task
    ↓
重新造一个 Provider
```

例如 `ConversationSyncProvider`：

```text
initial fetch
+
Tauri listen
+
running detection
+
setInterval polling
+
merge snapshot
+
Context
```

`SearchIndexProvider` 又来一次：

```text
initial fetch
+
Tauri listen
+
running detection
+
setInterval polling
+
state merge
+
Context
```

这说明你已经发现了：

```text
BackgroundTaskProvider
```

这个概念。

但是还没有进一步发现：

```text
BackgroundTaskRuntime
```

这个概念。

我更建议以后形成：

```text
             BackgroundTaskRuntime
                     │
        ┌────────────┼─────────────┐
        ↓            ↓             ↓
 Conversation      Memory      SearchIndex
 Task Adapter     Adapter       Adapter
```

Runtime 管：

```text
subscribe
poll
reconnect
refresh
task lifecycle
running/completed/failed
cleanup
```

Domain Adapter 管：

```text
event name
fetch task
start command
task normalization
domain-specific result
```

这样以后新增：

```text
ACP install task
Agent import task
Conversation export
Memory rebuild
Index build
Backup
```

就不再需要新建一整套 Provider。

这和你之前讨论 Pi/DSH 时的思想其实很像：

> **稳定机制下沉，变化能力做 Adapter。**

---

# 5. P1：甚至 Tauri Service Boundary 也只重构了一半

这个非常值得你注意。

现在 Provider 发 command 时：

```ts
import {
  listConversationSyncTasks,
  syncConversations,
} from "../../services/conversations";
```

很好。

意味着：

```text
React
↓
Service
↓
Tauri
```

但接事件时却直接：

```ts
import { listen } from "@tauri-apps/api/event";

listen(
  "conversation-sync-task-updated",
  ...
)
```

SearchIndex 也是：

```ts
import { listen } from "@tauri-apps/api/event";

const SEARCH_INDEX_TASK_UPDATED_EVENT =
  "conversation-search-index-task-updated";
```

于是 architecture 变成：

```text
Command:

React
 ↓
services
 ↓
Tauri


Event:

React Provider
 ↓
Tauri
```

这其实也是一条“双轨”。

更干净应该是：

```text
React
 ↓
Application / Hook
 ↓
Service / Infrastructure Port
 ↓
Tauri
```

Service 同时提供：

```ts
syncConversations()

listConversationSyncTasks()

subscribeConversationSyncTasks(callback)
```

Provider 根本不应该知道：

```text
"conversation-sync-task-updated"
```

这种 Tauri protocol detail。

这是一个很典型的**抽象边界只包住了 request，没有包住 event**。

---

# 6. P1：Controller 思路已经出现，但只在部分领域执行得比较彻底

Catalog 是明显的新架构。

你现在有：

```ts
useCatalogController()
```

里面继续组合：

```text
useCatalogData
useCatalogOperations
useTenantController
useExpandedAssets
useMountSelection
useAssetFilter
```

这个方向很好：

```text
Page
 ↓
Controller
 ↓
Domain Hooks
 ↓
Services
```

但是其他区域还没有全部采用这种思路。

最突出的就是：

```text
GlobalSettingsDialog.tsx
```

目前文件大小已经达到 **102,743 bytes**。与此同时它已经拆出 `AgentSettingsPanel` 等新组件，但整个 Settings 主体仍然是一个巨大的 smart component。

Conversation 更明显：

```text
ConversationsPage.tsx
106,692 bytes
```

也就是说你已经开始：

```text
Component decomposition
Hooks
Controller
Provider
```

但最早、最复杂的几个核心页面还保持：

```text
Mega Smart Component
```

### 这非常像典型的演进过程

```text
第一代：
Page = everything

第二代：
拆 Component

第三代：
拆 Hooks

第四代：
Controller / Application Layer

现在 AssetIWeave：
Catalog 已经比较接近第四代
Settings / Conversations 大约还在第二～三代
```

这正是你问的：

> “一部分用了重构后的新思路，一部分还没应用吗？”

**答案是非常明显地存在。**

---

# 7. P2：Settings Persistence 也是“双存储路径”

`AppSettingsProvider` 初始化先：

```ts
useState(() => readStoredSettings())
```

读取：

```text
localStorage
```

然后启动后又：

```ts
getAppSettings()
```

读取 Desktop JSON。

变化时又同时：

```ts
writeStoredSettings(settings)
saveAppSettings(settings)
```

而代码自己已经注明：

```ts
// The desktop JSON settings file remains the source of truth when available.
```

这不一定是 bug。

可能你的设计就是：

```text
Desktop JSON = authoritative store
localStorage = startup cache / fallback
```

如果明确如此，是可以成立的。

但目前从实现看，它仍然属于一个很典型的：

> **Web-era storage + Desktop-era storage 共存。**

因此需要给它一个正式的 contract。

例如明确：

```text
Canonical:
settings.json

Cache:
localStorage

Bootstrap:
localStorage → immediate UI
        ↓
settings.json → authoritative reconciliation

Write:
settings.json success
        ↓
update cache
```

而不是让两边看起来都是 write target。

否则以后很容易出现：

```text
localStorage 成功
JSON 写失败

或者：

JSON 被其他进程修改
localStorage 还是旧值
```

这种 split-brain。

---

# 我现在怎么看整个前端

如果画成“演进程度地图”，大概是：

```text
                     成熟程度

Theme / Recipe        █████████░
Catalog Controller    ████████░░
Route Loader          ███████░░░
Background Task       ███████░░░
UI Foundation         ██████░░░░
Modal System          █████░░░░░
Settings Architecture █████░░░░░
Conversation Page     ████░░░░░░
```

这不是说 Conversation 功能差，而是说**架构收口程度**。

---

# 最值得注意的是：这些问题其实有同一个模式

你现在不是存在七个孤立的问题。

它们高度一致：

```text
Radix Dialog
已经有了
→ 旧 Settings 没迁完


ui primitives
已经有了
→ foundation 里还有 SurfaceButton


routeLoaders
已经有了
→ AppRouter 还保留 direct lazy + giant switch


BackgroundTask Provider
已经有了
→ common runtime 没再往下抽


services
已经有了
→ Tauri event subscription 没进入 service


Controller pattern
已经有了
→ Settings / Conversations 没全面采用


Desktop JSON settings
已经有了
→ localStorage 旧路径仍然共同写入
```

所以我会把 AssetIWeave 现在的主要前端架构问题概括为：

> **不是缺少架构，而是架构演进速度很快，导致多个“新架构已经诞生、旧架构尚未退场”的 Migration Seam。**

这其实比“没有架构”好处理很多。

---

## 我建议不要分别搞 7 次大型重构

应该设一条非常明确的原则：

```text
以后出现新 abstraction：

1. 定义 Canonical Path
2. 新代码禁止再走 Legacy Path
3. 逐步迁移旧代码
4. 加 architecture guard
5. 删除 Legacy Path
```

例如：

```text
Modal:
Canonical = Radix-based foundation

Button:
Canonical = components/ui/Button

Route:
Canonical = route registry + loader

Task:
Canonical = BackgroundTaskRuntime

Tauri:
Canonical = services/infrastructure boundary

Feature logic:
Canonical = controller/hooks

Settings persistence:
Canonical = Desktop JSON
```

然后通过 ESLint / dependency boundary / test guard 去阻止旧路径重新增长。

**这最后一步非常重要。**

否则 AI Agent 特别容易看到附近旧代码后继续复制：

```text
这里以前是这样写的
→ 那我也这样写
```

你的代码库就会不断把已经重构掉的架构重新长回来。

如果让我排下一阶段的顺序，我会是：

**Modal 收口 → UI Primitive 收口 → BackgroundTask Runtime → Tauri transport boundary → Router 收口 → Settings/Conversation mega-component 拆分 → Settings persistence。**

其中前三项的特点都是：**改动相对可控，但能显著降低未来新增代码继续分叉的概率。**
