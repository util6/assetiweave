# Task 03：滚动活动检测与共享调度器

## 1. Objective

为每个复杂内容滚动容器建立一个稳定的 Scroll Activity Controller 和一个共享 Render Scheduler。业务内容和 Skeleton Boundary 只能订阅 phase 或排队，不得各自监听 scroll。

依赖：Task 02 的 `RenderSafeScrollSurface` 和可复用 ref。

## 2. Module structure

```text
frontend/src/components/common/rendering/
├── ScrollActivityController.ts
├── RenderScheduler.ts
├── RenderActivityProvider.tsx
├── renderingConstants.ts
└── *.test.ts(x)
```

## 3. ScrollActivityController contract

```ts
export type ScrollPhase = "idle" | "moving" | "fast";

export interface ScrollActivitySnapshot {
  direction: "backward" | "forward" | null;
  phase: ScrollPhase;
  velocity: number;
}

export interface ScrollActivityController {
  attach(element: HTMLElement): () => void;
  getSnapshot(): ScrollActivitySnapshot;
  subscribe(listener: () => void): () => void;
}

export function createScrollActivityController(): ScrollActivityController;
```

### 3.1 Constants

```ts
export const FAST_SCROLL_ENTER_PX_PER_MS = 1.25;
export const FAST_SCROLL_EXIT_PX_PER_MS = 0.6;
export const SCROLL_IDLE_DELAY_MS = 140;
export const VELOCITY_PREVIOUS_WEIGHT = 0.7;
export const VELOCITY_INSTANT_WEIGHT = 0.3;
```

### 3.2 Sampling algorithm

1. `attach()` 在 element 上添加一个 passive `scroll` listener。
2. 第一个 scroll event 记录当前 offset，并确保只有一个 sampling RAF。
3. RAF 读取最新 `scrollTop` 和 `performance.now()`。
4. 计算：

```ts
const instantVelocity = Math.abs(offset - previousOffset) / Math.max(now - previousTime, 1);
const velocity = previousVelocity * 0.7 + instantVelocity * 0.3;
```

5. `offset > previousOffset` 为 `forward`，反之为 `backward`。
6. `velocity >= 1.25` 进入 fast。
7. 当前 fast 且 `velocity > 0.6` 时继续保持 fast。
8. 非 fast 且有滚动事件时为 moving。
9. 最后一个 scroll event 后 140ms 进入 idle，并把 direction 置为 null、velocity 置为 0。
10. 仅在 Snapshot 的公开值实际变化时通知 subscribers。

### 3.3 Lifecycle

- 同一 Controller 同时只能 attach 一个 element。
- 重复 attach 新 element 前必须清理旧 listener、RAF 和 idle timer。
- React StrictMode mount/unmount/mount 不得产生重复 listener。
- 页面卸载后不得继续回调 subscriber。

## 4. RenderScheduler contract

```ts
export type RenderPriority = 0 | 1 | 2;

export interface ScheduledRenderTask {
  commit: () => void;
  key: string;
  priority: RenderPriority;
}

export interface RenderScheduler {
  cancel(key: string): void;
  dispose(): void;
  schedule(task: ScheduledRenderTask): () => void;
  setPhase(phase: ScrollPhase): void;
  size(): number;
}

export function createRenderScheduler(options?: {
  onError?: (error: unknown, key: string) => void;
}): RenderScheduler;
```

Priority 定义：

| Priority | Meaning |
|---|---|
| 0 | viewport 内 |
| 1 | 滚动方向前方 Overscan |
| 2 | 后方 Overscan |

### 4.1 Queue behavior

- `key` 唯一；重复 schedule 同一个 key 时更新 priority 和 commit，不增加第二条记录。
- cancel 必须幂等。
- commit 执行前自动从队列移除。
- commit 抛错时必须从队列移除、调用 `onError(error, key)` 并继续处理其他任务；禁止在 RAF 中留下未处理异常。
- dispose 清空队列、取消 RAF、禁止新提交。

### 4.2 Phase budget

```text
fast   -> 不创建 flush RAF，不提交任务
moving -> 每个 RAF 最多提交 1 个任务
idle   -> 每个 RAF 最多提交 4 个，且累计执行时间最多 4ms
```

排序：

1. priority 升序。
2. 同 priority 按首次入队顺序。

不得使用 `requestIdleCallback`，避免依赖不同 WebKit 版本的实现差异。统一使用 `requestAnimationFrame + performance.now()` 帧预算。

## 5. React provider contract

```ts
export interface RenderActivityProviderProps {
  children: React.ReactNode;
  scrollElementRef: React.RefObject<HTMLElement | null>;
}

export function RenderActivityProvider(
  props: RenderActivityProviderProps,
): React.ReactElement;

export function useRenderActivity(): {
  controller: ScrollActivityController;
  scheduler: RenderScheduler;
};

export function useScrollActivitySnapshot(): ScrollActivitySnapshot;
```

实现要求：

- Context 中保存稳定 Controller/Scheduler 对象，不直接保存每帧 velocity state。
- `useScrollActivitySnapshot` 使用 `useSyncExternalStore`。
- Provider 监听 phase 变化并调用 `scheduler.setPhase(phase)`。
- Phase 变化时将 scroll element 的 `data-scroll-phase` 设置为 `idle/moving/fast`。
- 不因 velocity 每帧变化导致整个 Conversation Preview React 重渲染；只有使用 Snapshot 的 Boundary 会更新。

## 6. Shimmer control

增加统一规则：

```css
[data-scroll-phase="fast"] .aurora-skeleton::after {
  animation: none;
  opacity: 0;
}
```

要求：

- 规则位于统一 Skeleton/Foundation 样式区域。
- moving 和 idle 继续遵循统一 Skeleton SPEC。
- `prefers-reduced-motion` 仍拥有更高或等价约束。
- 不修改非 Skeleton 的状态 pulse。

## 7. Tests

ScrollActivityController 使用 fake RAF、fake timer 和可控 `performance.now()`，至少覆盖：

- attach 只安装一个 listener。
- 低速滚动进入 moving。
- 超过 enter threshold 进入 fast。
- hysteresis：速度在 0.6–1.25 之间时保持当前 fast。
- 低于 exit threshold 回到 moving。
- 140ms 无事件进入 idle。
- forward/backward 正确。
- cleanup 取消 listener、RAF、timer。

RenderScheduler 至少覆盖：

- fast 不提交。
- moving 每帧一个。
- idle priority 顺序正确。
- idle 不突破 4 个任务。
- key 去重。
- cancel 和 dispose 幂等。
- phase 从 fast 变 idle 后开始 flush。

Provider 至少覆盖：

- StrictMode 不重复 attach。
- `data-scroll-phase` 正确更新。
- unmount dispose。

## 8. Files likely touched

```text
frontend/src/components/common/rendering/ScrollActivityController.ts
frontend/src/components/common/rendering/RenderScheduler.ts
frontend/src/components/common/rendering/RenderActivityProvider.tsx
frontend/src/components/common/rendering/renderingConstants.ts
frontend/src/components/common/rendering/*.test.ts(x)
frontend/src/styles/index.css
```

本任务不接入 Conversations 真实内容；只允许用测试 Harness 验证 Provider。

## 9. Acceptance criteria

- [ ] 每个 Provider 只有一个 scroll listener 和一个 Scheduler。
- [ ] phase 算法、阈值和 hysteresis 与总 SPEC 一致。
- [ ] fast 阶段无新 commit。
- [ ] moving/idle 符合帧预算。
- [ ] shimmer 在 fast 阶段停止。
- [ ] StrictMode 生命周期无泄漏。
- [ ] 全部单元测试通过。

## 10. Verification

```bash
pnpm vitest run --config frontend/vite.config.ts \
  frontend/src/components/common/rendering/ScrollActivityController.test.ts \
  frontend/src/components/common/rendering/RenderScheduler.test.ts \
  frontend/src/components/common/rendering/RenderActivityProvider.test.tsx
pnpm typecheck
```

## 11. Commit

```text
feat: 增加滚动活动检测与共享渲染调度器
```
