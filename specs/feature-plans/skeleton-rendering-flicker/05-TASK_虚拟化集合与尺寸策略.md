# Task 05：VirtualizedCollection 与粗粒度尺寸策略

## 1. Objective

基于 TanStack Virtual 建立跨业务可复用的纵向虚拟集合，控制长复杂内容的挂载数量，并自动使用 Deferred Skeleton Boundary 填充尚未提交的项目。

依赖：Tasks 01–04 完成。

## 2. Dependency installation

```bash
pnpm add @tanstack/react-virtual@^3.14.9
```

要求：

- 提交 `package.json` 和 `pnpm-lock.yaml`。
- 不安装 MUI、Chakra 或其他 Skeleton 包。
- 生产构建后记录新增 gzip bundle 体积。
- 如果实际解析版本高于 3.14.x 且 API 不兼容，停止实现并更新本 SPEC，不得猜测 API。

## 3. Component location

```text
frontend/src/components/common/rendering/VirtualizedCollection.tsx
```

该组件属于 cross-domain composite，不放入 Foundation Skeleton 目录。

## 4. Public contract

```ts
export interface VirtualizedCollectionHandle {
  measure(): void;
  scrollToKey(
    key: string,
    options?: {
      align?: "auto" | "center" | "end" | "start";
      behavior?: "auto" | "smooth";
    },
  ): boolean;
}

export interface VirtualizedCollectionProps<Item> {
  className?: string;
  eagerKeys?: ReadonlySet<string>;
  enabled?: boolean;
  estimateSize?: number | ((item: Item, index: number) => number);
  fallback?: (item: Item, index: number) => React.ReactNode;
  gap?: number;
  getItemKey: (item: Item, index: number) => string;
  items: readonly Item[];
  minItems?: number;
  onItemReady?: (key: string) => void;
  pinnedKeys?: ReadonlySet<string>;
  renderItem: (item: Item, index: number) => React.ReactNode;
  scrollElementRef: React.RefObject<HTMLElement | null>;
  size?: SkeletonBlockSize;
}
```

默认值：

```ts
enabled = true
estimateSize = SKELETON_BLOCK_SIZE_PX[size ?? "regular"]
gap = 24
minItems = 12
size = "regular"
```

## 5. Enablement behavior

```ts
const shouldVirtualize = enabled && items.length >= minItems;
```

### Disabled or short collection

- 直接按正常文档流渲染全部项目。
- 每个项目仍使用 `DeferredSkeletonBoundary`。
- Boundary 通过共享 Visibility Registry 获得 priority。
- 适用于短列表和 feature flag 回滚。

### Enabled long collection

- 使用 `useVirtualizer`。
- 只渲染 virtual range、Overscan 和 pinned keys。
- 每个 Virtual Item 自动包装 Deferred Skeleton Boundary。

## 6. TanStack Virtual configuration

必须等价于：

```tsx
const virtualizer = useVirtualizer({
  count: items.length,
  estimateSize: (index) => resolveEstimateSize(items[index], index),
  getItemKey: (index) => getItemKey(items[index], index),
  getScrollElement: () => scrollElementRef.current,
  gap,
  overscan: overscanForPhase(scrollPhase),
  rangeExtractor: pinnedRangeExtractor,
  useScrollendEvent: false,
  isScrollingResetDelay: SCROLL_IDLE_DELAY_MS,
});
```

规则：

- `getItemKey` 必须返回非空稳定 string。
- 开发和测试环境发现重复 key 时抛出错误。
- 禁止回退到 index key。
- 使用 `virtualizer.measureElement` 测量真实 wrapper。
- 不同时对同一 item 使用 `resizeItem` 和 `measureElement`。
- v1 使用默认 start anchor，不使用 chat end anchoring。
- 程序化定位默认 `behavior="auto"`；动态测量列表不默认使用 smooth。

## 7. DOM structure

```tsx
<div
  className={cn("virtualized-collection", className)}
  data-virtualized-collection=""
  style={{ height: virtualizer.getTotalSize(), position: "relative" }}
>
  {virtualizer.getVirtualItems().map((virtualItem) => {
    const item = items[virtualItem.index];
    const key = getItemKey(item, virtualItem.index);

    return (
      <div
        aria-posinset={virtualItem.index + 1}
        aria-setsize={items.length}
        data-index={virtualItem.index}
        data-virtual-item-key={key}
        key={virtualItem.key}
        ref={virtualizer.measureElement}
        style={{
          left: 0,
          position: "absolute",
          top: 0,
          transform: `translateY(${virtualItem.start}px)`,
          width: "100%",
        }}
      >
        <DeferredSkeletonBoundary
          forceReady={eagerKeys?.has(key)}
          fallback={fallback?.(item, virtualItem.index)}
          itemKey={key}
          onReady={onItemReady}
          priority={priorityForVirtualItem(virtualItem)}
          size={size}
        >
          {renderItem(item, virtualItem.index)}
        </DeferredSkeletonBoundary>
      </div>
    );
  })}
</div>
```

实现可以提取 helper，但必须保留可测的 data attributes。

## 8. Overscan

```ts
export function overscanForPhase(phase: ScrollPhase): number {
  if (phase === "fast") return 8;
  if (phase === "moving") return 5;
  return 3;
}
```

Priority 计算：

- Virtual Item 与 `scrollRect` 相交：0。
- 不相交且位于当前滚动方向前方：1。
- 其他 Overscan：2。

Overscan 的真实内容是否提交仍由 Scheduler 决定。增加 Overscan 不得绕过 Boundary。

## 9. Pinned keys

`pinnedKeys` 用于保证以下项目保持挂载：

- 内部当前拥有焦点。
- 包含正在进行且不能丢失本地 UI 连接的操作。
- 当前 search/navigation target 所属项目。

实现：

- 建立 `key -> index` Map，items 变化时用 `useMemo` 重建。
- `rangeExtractor` 合并默认 range 和 pinned index。
- 返回升序去重 indexes。
- 默认最多允许 4 个 pinned key；超过时在开发环境警告并只保留调用方集合迭代顺序的前 4 个。
- 不存在的 key 被忽略。

Pinned 不是状态外置的替代；Task 06 仍必须移动 Conversations 持久交互状态。

`eagerKeys` 是 pinned 的更强形式：

- eager key 必须同时合并进 pinned range。
- eager item 的 Boundary 设置 `forceReady=true`。
- 用于主动搜索定位或必须立即恢复焦点的项目。
- 默认最多允许 2 个 eager key；超过时只保留集合迭代顺序前 2 个并在开发环境警告。
- 普通 Overscan 禁止加入 eagerKeys。

## 10. Imperative navigation

`scrollToKey`：

1. 通过 key map 找到 index。
2. 不存在返回 false，不抛错。
3. 存在时调用 `virtualizer.scrollToIndex(index, options)` 并返回 true。
4. 非虚拟模式下通过 `[data-virtual-item-key]` 找元素并调用 `scrollIntoView`。
5. 默认 `{ align: "center", behavior: "auto" }`。

Conversations search target 必须先定位所属 Turn，再调用 `scrollToKey(turnId)`；不能继续假设目标 Card DOM 已经挂载。

## 11. Size measurement

- Skeleton 阶段 wrapper 由 coarse estimate 占位。
- Boundary ready 后 ResizeObserver/`measureElement` 更新 Virtualizer 内部 measurement。
- v1 不把 measurement 暴露为全局业务 API。
- ResizableColumns 改变宽度后调用 handle `measure()`。
- 字体、主题或全局 typography 设置变化后调用 `measure()`。
- 不在 render 中同步读取所有 item DOM 高度。

## 12. Accessibility and focus

- wrapper 提供 `aria-posinset` 和 `aria-setsize`。
- 不擅自增加 `role="list"`；由调用方真实语义决定。
- 获得焦点的 item 必须被加入 pinnedKeys。
- 如果 focus item 即将被移除，先将其 pin，而不是把焦点移动到 body。
- Keyboard PageUp/PageDown、Home/End 后 Scheduler 和 Skeleton 行为与指针滚动一致。
- Export、Copy all 和 Search 数据操作必须继续基于完整内存数据，而不是当前挂载 DOM。

## 13. Tests

需要 stub `ResizeObserver`、scroll element rect 和 offset，覆盖：

- 少于 12 项走非虚拟路径。
- 12 项及以上启用 Virtualizer。
- 同时挂载数量受 range + overscan 限制。
- fast/moving/idle overscan 分别为 8/5/3。
- stable key 透传。
- 重复/空 key 在开发测试环境报错。
- fallback 和 renderItem 分别进入 Skeleton/ready 状态。
- `measureElement` 被绑定。
- pinned key 合并到 range。
- pinned key 超过 4 个被限制。
- eager key 强制挂载并立即 ready，且最多 2 个。
- `onItemReady` 在项目首次 ready 时收到稳定业务 key。
- `scrollToKey` 存在/不存在两条路径。
- enabled=false 回退到完整文档流。
- item reorder 后 key 仍关联正确业务对象。

## 14. Files likely touched

```text
package.json
pnpm-lock.yaml
frontend/src/components/common/rendering/VirtualizedCollection.tsx
frontend/src/components/common/rendering/VirtualizedCollection.test.tsx
frontend/src/components/common/rendering/index.ts
```

## 15. Acceptance criteria

- [ ] 正式使用 TanStack Virtual，没有自研 range 算法。
- [ ] 短集合自动回退正常 DOM 流。
- [ ] 长集合挂载数受控。
- [ ] 所有 virtual item 自动经过 Deferred Boundary。
- [ ] Overscan 随 phase 调整。
- [ ] 稳定 key、pin、measure 和 scrollToKey 可用。
- [ ] 无 index key、无双重测量。
- [ ] Bundle 增量已记录。

## 16. Verification

```bash
pnpm vitest run --config frontend/vite.config.ts \
  frontend/src/components/common/rendering/VirtualizedCollection.test.tsx
pnpm typecheck
pnpm build
pnpm artifacts:check
```

## 17. Commit

```text
feat: 增加骨架驱动的虚拟化集合
```
