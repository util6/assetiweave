# Task 04：延迟 Skeleton 渲染边界 (Deferred Skeleton Render Boundary)

## 1. Objective

实现一个与内容类型无关的通用渲染边界：未获得提交许可时显示统一 Skeleton，进入 viewport/Overscan 且滚动预算允许后提交原始 React children。

依赖：

- 统一 Skeleton SPEC 已实现。
- Task 03 的 Provider、Scroll Activity 和 Scheduler 已完成。

## 2. Component contract

新增：

```text
frontend/src/components/common/rendering/DeferredSkeletonBoundary.tsx
```

接口：

```ts
import type { RenderPriority } from "./RenderScheduler";
import type { SkeletonBlockSize } from "./renderingTypes";

export interface DeferredSkeletonBoundaryProps {
  children: React.ReactNode;
  className?: string;
  enabled?: boolean;
  fallback?: React.ReactNode;
  forceReady?: boolean;
  itemKey: string;
  onReady?: (itemKey: string) => void;
  priority?: RenderPriority;
  size?: SkeletonBlockSize;
}

export function DeferredSkeletonBoundary(
  props: DeferredSkeletonBoundaryProps,
): React.ReactElement;
```

默认值：

```ts
enabled = true
forceReady = false
size = "regular"
```

## 3. Default fallback

未提供 fallback 时使用统一 Foundation：

```tsx
function DefaultDeferredSkeleton({ size }: { size: SkeletonBlockSize }) {
  return (
    <SkeletonSurface className="deferred-render-skeleton">
      <Skeleton className="h-4 w-2/5" />
      <SkeletonText lines={3} />
      {size === "tall" ? <Skeleton className="h-24 w-full" /> : null}
    </SkeletonSurface>
  );
}
```

规则：

- fallback 只是几何占位，不拥有 `role="status"`。
- 外层页面 loading label 由统一 Skeleton 或页面已有语义提供。
- 滚动占位属于视觉性能机制，不应在每个条目向屏幕阅读器播报“加载中”。
- fallback 必须 `aria-hidden="true"`。

## 4. Size bucket

```ts
export const SKELETON_BLOCK_SIZE_PX = {
  compact: 96,
  regular: 224,
  tall: 420,
} as const;

export type SkeletonBlockSize = keyof typeof SKELETON_BLOCK_SIZE_PX;
```

Boundary 根节点设置：

```tsx
style={{
  "--render-estimated-block-size": `${SKELETON_BLOCK_SIZE_PX[size]}px`,
} as React.CSSProperties}
```

CSS：

```css
.deferred-render-boundary {
  min-block-size: var(--render-estimated-block-size);
  contain: layout paint style;
  background: rgb(var(--color-background));
}

.deferred-render-skeleton {
  min-block-size: var(--render-estimated-block-size);
}

.deferred-render-boundary[data-render-state="ready"] {
  min-block-size: 0;
}
```

禁止根据内容类型自动选择 size。Feature integration 明确选择即可。

## 5. Visibility registration

为了避免每个 Boundary 创建一个 IntersectionObserver，Task 04 必须在 Provider 中增加共享 Visibility Registry：

```ts
export interface RenderVisibilityRegistration {
  element: HTMLElement;
  key: string;
  onPriorityChange: (priority: RenderPriority | null) => void;
}

export interface RenderVisibilityRegistry {
  register(registration: RenderVisibilityRegistration): () => void;
}
```

实现规则：

- 每个 scroll surface 只有一个 IntersectionObserver。
- `root` 是 Provider 的 scroll element。
- `rootMargin` 为上下各一个 viewport：读取 scroll element `clientHeight` 后使用 `${clientHeight}px 0px`。
- Provider 使用一个共享 ResizeObserver 观察 scroll element；高度变化时重建共享 IntersectionObserver，不允许每个 Boundary 自建 observer。
- entry 与真实 rootBounds 相交时 priority 0。
- 只与扩展 rootMargin 相交时，根据滚动方向分配 1 或 2。
- 完全离开 rootMargin 时返回 null 并取消 queued task。
- VirtualizedCollection 如果显式传入 `priority`，Boundary 不注册 Observer。
- 判断真实 viewport 必须使用 scroll element 的 `getBoundingClientRect()`；IntersectionObserver 的扩展 rootBounds 只用于 Overscan 命中，不能直接当成真实 viewport。

## 6. State machine implementation

组件内部状态：

```ts
const [state, setState] = useState<DeferredRenderState>(
  enabled ? "skeleton" : "ready",
);
```

规则：

1. `enabled=false` 或 `forceReady=true` 时立即渲染 children，不注册可见性、不排队。
2. `state=ready` 时永远渲染 children，直到组件被卸载。
3. priority 为 null 时保持 skeleton，并取消尚未提交的 task。
4. priority 非 null 且 phase 非 fast 时进入 queued，并调用 Scheduler。
5. phase 变 fast 时：
   - skeleton 保持 skeleton。
   - queued 保持 queued，但 Scheduler 不提交。
   - ready 保持 ready。
6. Scheduler commit 时确认组件仍 mounted、itemKey 未变化，然后设为 ready。
7. itemKey 变化时重置为 skeleton，并取消旧 key task。
8. unmount 时取消 task 和 visibility registration。
9. 首次进入 ready 后调用一次 `onReady(itemKey)`；普通 re-render 不得重复调用。

任务 key 格式：

```ts
`deferred-render:${itemKey}`
```

itemKey 必须在一个 Provider 内唯一。

## 7. Rendering contract

Skeleton 状态：

```tsx
<div
  className="deferred-render-boundary"
  data-render-item-key={itemKey}
  data-render-state="skeleton"
>
  {fallback ?? <DefaultDeferredSkeleton size={size} />}
</div>
```

Ready 状态：

```tsx
<div
  className="deferred-render-boundary render-safe-content"
  data-render-item-key={itemKey}
  data-render-state="ready"
>
  {children}
</div>
```

禁止：

- 使用 `display: contents`，因为 Boundary 需要 containment 和测量节点。
- 同时把 fallback 和 children 放入 DOM 后仅用 opacity 切换。
- 在 fast phase 把 ready 内容替换回 Skeleton。
- 在 Boundary 内检查 Markdown、renderer 或 Card kind。

## 8. 焦点与交互安全性 (Focus & Interaction Safety)

- 如果 Boundary 内当前拥有 `document.activeElement`，VirtualizedCollection 不应将该 item 排除；Task 05 必须提供 pinned key 支持。
- Boundary 自身不可 focus。
- Skeleton 不得包含交互元素。
- ready 后保留正常 tab order。
- `forceReady` 只用于主动导航目标、当前焦点项等少量高优先级项目；普通 Overscan 禁止使用。
- 业务状态不能只存放在可能被 Virtualizer 卸载的组件内部；Conversations 由 Task 06 处理。

## 9. Tests

必须使用真实 Provider fake controller/scheduler 或小型确定性 fake，覆盖：

- 默认首先渲染 Skeleton。
- idle + priority 0 排队并 ready。
- fast 不 ready。
- queued 在 phase 变 idle 后 ready。
- ready 后 phase 变 fast 仍保持 children。
- 离开 Overscan 取消 queued task。
- unmount 后 commit 不触发 state update。
- itemKey 变化重置。
- enabled=false 立即 children。
- forceReady=true 即使 fast 也立即 children。
- onReady 每次 itemKey 挂载生命周期只调用一次。
- 自定义 fallback 生效。
- 默认 fallback 使用统一 Skeleton Primitive。
- 一个 Provider 只有一个 IntersectionObserver。

## 10. Files likely touched

```text
frontend/src/components/common/rendering/DeferredSkeletonBoundary.tsx
frontend/src/components/common/rendering/RenderVisibilityRegistry.ts
frontend/src/components/common/rendering/renderingTypes.ts
frontend/src/components/common/rendering/*.test.ts(x)
frontend/src/components/common/rendering/RenderActivityProvider.tsx
frontend/src/styles/index.css
```

## 11. Acceptance criteria

- [ ] Boundary 完全不感知内容类型。
- [ ] 未 ready 始终有 Skeleton，不出现空 DOM。
- [ ] fast 阶段不提交新内容。
- [ ] ready 内容不会主动降级。
- [ ] 每个 Provider 只有一个 IntersectionObserver。
- [ ] cleanup、itemKey 变化和 StrictMode 正确。
- [ ] 默认 fallback 使用统一 Skeleton 架构。

## 12. Verification

```bash
pnpm vitest run --config frontend/vite.config.ts \
  frontend/src/components/common/rendering/DeferredSkeletonBoundary.test.tsx \
  frontend/src/components/common/rendering/RenderVisibilityRegistry.test.ts
pnpm typecheck
pnpm build
```

## 13. Commit

```text
feat: 增加通用骨架渲染边界
```
