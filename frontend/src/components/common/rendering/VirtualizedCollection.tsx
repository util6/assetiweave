import {
  defaultRangeExtractor,
  useVirtualizer,
  type Range,
  type VirtualItem,
} from "@tanstack/react-virtual";
import {
  forwardRef,
  useCallback,
  useImperativeHandle,
  useMemo,
} from "react";
import type { CSSProperties, ReactNode, RefObject } from "react";
import { cn } from "../../../lib/utils";
import { DeferredSkeletonBoundary } from "./DeferredSkeletonBoundary";
import { useScrollActivitySnapshot } from "./RenderActivityProvider";
import { SCROLL_IDLE_DELAY_MS } from "./renderingConstants";
import type { RenderPriority } from "./RenderScheduler";
import { SKELETON_BLOCK_SIZE_PX, type SkeletonBlockSize } from "./renderingTypes";

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
  fallback?: (item: Item, index: number) => ReactNode;
  gap?: number;
  getItemKey: (item: Item, index: number) => string;
  items: readonly Item[];
  minItems?: number;
  onItemReady?: (key: string) => void;
  pinnedKeys?: ReadonlySet<string>;
  renderItem: (item: Item, index: number) => ReactNode;
  scrollElementRef: RefObject<HTMLElement | null>;
  size?: SkeletonBlockSize;
}

export function overscanForPhase(phase: "idle" | "moving" | "fast"): number {
  if (phase === "fast") return 8;
  if (phase === "moving") return 5;
  return 3;
}

function limitKeys(
  keys: ReadonlySet<string> | undefined,
  limit: number,
  name: string,
): ReadonlySet<string> {
  if (!keys || keys.size <= limit) return keys ?? new Set<string>();
  const limited = new Set<string>();
  for (const key of keys) {
    if (limited.size >= limit) break;
    limited.add(key);
  }
  const env = (import.meta as ImportMeta & { env?: { DEV?: boolean } }).env;
  if (env?.DEV) console.warn(`${name} keys are limited to ${limit} items`);
  return limited;
}

function validateKeys<Item>(items: readonly Item[], getItemKey: VirtualizedCollectionProps<Item>["getItemKey"]): string[] {
  const keys = items.map((item, index) => getItemKey(item, index));
  const seen = new Set<string>();
  keys.forEach((key) => {
    if (typeof key !== "string" || key.trim().length === 0) {
      throw new Error("VirtualizedCollection requires non-empty string item keys");
    }
    if (seen.has(key)) throw new Error(`VirtualizedCollection duplicate item key: ${key}`);
    seen.add(key);
  });
  return keys;
}

function priorityForVirtualItem(
  item: VirtualItem,
  scrollElement: HTMLElement | null,
  direction: "backward" | "forward" | null,
): RenderPriority {
  if (!scrollElement) return 0;
  const scrollTop = scrollElement.scrollTop;
  const viewportBottom = scrollTop + scrollElement.clientHeight;
  if (item.start < viewportBottom && item.end > scrollTop) return 0;
  const isAhead = direction === "backward"
    ? item.end <= scrollTop
    : item.start >= viewportBottom;
  return isAhead ? 1 : 2;
}

function VirtualizedCollectionInner<Item>(
  {
    className,
    eagerKeys,
    enabled = true,
    estimateSize,
    fallback,
    gap = 24,
    getItemKey,
    items,
    minItems = 12,
    onItemReady,
    pinnedKeys,
    renderItem,
    scrollElementRef,
    size = "regular",
  }: VirtualizedCollectionProps<Item>,
  ref: React.ForwardedRef<VirtualizedCollectionHandle>,
) {
  const { direction, phase } = useScrollActivitySnapshot();
  const keys = useMemo(() => validateKeys(items, getItemKey), [getItemKey, items]);
  const keyToIndex = useMemo(
    () => new Map(keys.map((key, index) => [key, index] as const)),
    [keys],
  );
  const limitedPinnedKeys = useMemo(() => limitKeys(pinnedKeys, 4, "pinned"), [pinnedKeys]);
  const limitedEagerKeys = useMemo(() => limitKeys(eagerKeys, 2, "eager"), [eagerKeys]);
  const pinnedIndexes = useMemo(() => {
    const indexes = new Set<number>();
    for (const key of [...limitedPinnedKeys, ...limitedEagerKeys]) {
      const index = keyToIndex.get(key);
      if (index != null) indexes.add(index);
    }
    return indexes;
  }, [keyToIndex, limitedEagerKeys, limitedPinnedKeys]);
  const rangeExtractor = useMemo(
    () => (range: Range) => {
      const indexes = new Set(defaultRangeExtractor(range));
      pinnedIndexes.forEach((index) => indexes.add(index));
      return [...indexes].sort((left, right) => left - right);
    },
    [pinnedIndexes],
  );
  const resolveEstimateSize = useCallback(
    (index: number) => typeof estimateSize === "function"
      ? estimateSize(items[index]!, index)
      : estimateSize ?? SKELETON_BLOCK_SIZE_PX[size],
    [estimateSize, items, size],
  );
  const shouldVirtualize = enabled && items.length >= minItems;
  const virtualizer = useVirtualizer({
    count: items.length,
    enabled: shouldVirtualize,
    estimateSize: resolveEstimateSize,
    gap,
    getItemKey: (index) => keys[index]!,
    initialOffset: 0,
    initialRect: {
      height: scrollElementRef.current?.clientHeight ?? 0,
      width: scrollElementRef.current?.clientWidth ?? 0,
    },
    getScrollElement: () => scrollElementRef.current,
    isScrollingResetDelay: SCROLL_IDLE_DELAY_MS,
    overscan: overscanForPhase(phase),
    rangeExtractor,
    useScrollendEvent: false,
    useFlushSync: false,
  });

  useImperativeHandle(ref, () => ({
    measure: () => virtualizer.measure(),
    scrollToKey: (key, options = {}) => {
      const index = keyToIndex.get(key);
      if (index == null) return false;
      if (shouldVirtualize) {
        virtualizer.scrollToIndex(index, {
          align: options.align ?? "center",
          behavior: options.behavior ?? "auto",
        });
        return true;
      }
      const element = [...(scrollElementRef.current?.querySelectorAll<HTMLElement>("[data-virtual-item-key]") ?? [])]
        .find((candidate) => candidate.dataset.virtualItemKey === key);
      element?.scrollIntoView({
        block: options.align === "end" ? "end" : options.align === "start" ? "start" : "center",
        behavior: options.behavior ?? "auto",
      });
      return Boolean(element);
    },
  }), [keyToIndex, scrollElementRef, shouldVirtualize, virtualizer]);

  const renderBoundary = useCallback((item: Item, index: number, key: string, priority?: RenderPriority) => (
    <DeferredSkeletonBoundary
      forceReady={limitedEagerKeys.has(key)}
      fallback={fallback?.(item, index)}
      itemKey={key}
      onReady={onItemReady}
      priority={priority}
      size={size}
    >
      {renderItem(item, index)}
    </DeferredSkeletonBoundary>
  ), [fallback, limitedEagerKeys, onItemReady, renderItem, size]);

  if (!shouldVirtualize) {
    return (
      <div className={cn("virtualized-collection", className)} data-virtualized-collection="">
        {items.map((item, index) => {
          const key = keys[index]!;
          return (
            <div
              aria-posinset={index + 1}
              aria-setsize={items.length}
              data-index={index}
              data-virtual-item-key={key}
              key={key}
            >
              {renderBoundary(item, index, key)}
            </div>
          );
        })}
      </div>
    );
  }

  const virtualItems = virtualizer.getVirtualItems();
  return (
    <div
      className={cn("virtualized-collection", className)}
      data-virtualized-collection=""
      style={{ height: virtualizer.getTotalSize(), position: "relative" }}
    >
      {virtualItems.map((virtualItem) => {
        const item = items[virtualItem.index]!;
        const key = keys[virtualItem.index]!;
        const priority = priorityForVirtualItem(virtualItem, scrollElementRef.current, direction);
        return (
          <div
            aria-posinset={virtualItem.index + 1}
            aria-setsize={items.length}
            data-index={virtualItem.index}
            data-virtual-item-key={key}
            key={key}
            ref={virtualizer.measureElement}
            style={{
              left: 0,
              position: "absolute",
              top: 0,
              transform: `translateY(${virtualItem.start}px)`,
              width: "100%",
            } as CSSProperties}
          >
            {renderBoundary(item, virtualItem.index, key, priority)}
          </div>
        );
      })}
    </div>
  );
}

export const VirtualizedCollection = forwardRef(VirtualizedCollectionInner) as <Item>(
  props: VirtualizedCollectionProps<Item> & { ref?: React.ForwardedRef<VirtualizedCollectionHandle> },
) => React.ReactElement;
