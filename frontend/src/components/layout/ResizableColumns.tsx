import { ChevronLeft, ChevronRight } from "lucide-react";
import {
  Children,
  type CSSProperties,
  type KeyboardEvent,
  type PointerEvent as ReactPointerEvent,
  type ReactNode,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import {
  Group,
  Panel,
  Separator,
  type GroupImperativeHandle,
  type Layout,
  type LayoutChangedMeta,
} from "react-resizable-panels";
import { cn } from "../../lib/utils";
import { useAppSettings } from "../../store/settings/useAppSettings";
import {
  fromPanelLayout,
  sanitizeColumnWeights,
  toPanelLayout,
} from "./columnLayouts";

export interface ResizableColumnConfig {
  defaultWeight: number;
  minWidthScale?: number;
}

export interface ResizableColumnsProps {
  ariaLabel: string;
  children: ReactNode;
  className?: string;
  columns: ResizableColumnConfig[];
  handleClassName?: string;
  minimumWidth: number;
  responsiveClassName?: string;
  scrollBarLabel: string;
  scrollLeftLabel: string;
  scrollRightLabel: string;
  storageKey?: string;
}

export interface ScrollMetrics {
  clientWidth: number;
  scrollLeft: number;
  scrollWidth: number;
}

type ResizableColumnsStyle = CSSProperties & Record<`--${string}`, string>;

interface ScrollDragState {
  maxScroll: number;
  startClientX: number;
  startScrollLeft: number;
  trackTravelWidth: number;
}

const SCROLL_BUTTON_STEP = 160;

const EMPTY_SCROLL_METRICS: ScrollMetrics = {
  clientWidth: 0,
  scrollLeft: 0,
  scrollWidth: 0,
};

export function ResizableColumns({
  ariaLabel,
  children,
  className,
  columns,
  handleClassName,
  minimumWidth,
  responsiveClassName,
  scrollBarLabel,
  scrollLeftLabel,
  scrollRightLabel,
  storageKey,
}: ResizableColumnsProps) {
  const { settings, settingsLoaded, settingsConfirmed, setColumnLayoutAsync } =
    useAppSettings();

  const fallbackWeights = useMemo(
    () => columns.map((column) => column.defaultWeight),
    [columns],
  );
  const minWidths = useMemo(
    () => resolveColumnMinWidths(minimumWidth, columns),
    [columns, minimumWidth],
  );
  const totalMinimumWidth = minWidths.reduce((sum, width) => sum + width, 0);
  const resizableCanvasWidth =
    totalMinimumWidth + Math.round(minimumWidth * 0.5);

  const [weights, setWeights] = useState<number[]>(() => {
    if (storageKey && settings.columnLayouts?.[storageKey]) {
      const stored = settings.columnLayouts[storageKey];
      if (Array.isArray(stored) && stored.length === fallbackWeights.length) {
        return sanitizeColumnWeights(stored, fallbackWeights);
      }
    }
    return readStoredColumnWeights(storageKey, fallbackWeights);
  });

  const [scrollDragState, setScrollDragState] =
    useState<ScrollDragState | null>(null);
  const [scrollMetrics, setScrollMetrics] =
    useState<ScrollMetrics>(EMPTY_SCROLL_METRICS);

  const groupRef = useRef<GroupImperativeHandle>(null);
  const groupElementRef = useRef<HTMLDivElement | null>(null);
  const lastCommittedWeightsRef = useRef<number[]>(weights);
  const migratedStorageKeyRef = useRef<string | null>(null);

  const scrollTrackRef = useRef<HTMLDivElement>(null);
  const scrollViewportRef = useRef<HTMLDivElement>(null);
  const childArray = Children.toArray(children);

  // Settings 异步到达后同步到 Group 布局
  useEffect(() => {
    if (!storageKey || !settingsLoaded) return;
    const stored = settings.columnLayouts?.[storageKey];
    if (!stored || stored.length !== columns.length) return;
    const sanitized = sanitizeColumnWeights(stored, fallbackWeights);
    if (!arraysAlmostEqual(sanitized, weights)) {
      setWeights(sanitized);
      lastCommittedWeightsRef.current = sanitized;
      groupRef.current?.setLayout(toPanelLayout(sanitized));
    }
  }, [
    storageKey,
    settingsLoaded,
    settings.columnLayouts,
    columns.length,
    fallbackWeights,
    weights,
  ]);

  // 旧 localStorage 权重迁移至 SQLite 设置（必须等待后端数据确认到达，禁止在启动缓存阶段回写）
  useEffect(() => {
    if (!storageKey || !settingsConfirmed) return;
    if (migratedStorageKeyRef.current === storageKey) return;

    const existingInSettings = settings.columnLayouts?.[storageKey];
    if (existingInSettings && existingInSettings.length === columns.length) {
      return;
    }

    if (typeof localStorage === "undefined") return;
    try {
      const raw = localStorage.getItem(storageKey);
      if (!raw) return;
      const parsed = JSON.parse(raw);
      if (Array.isArray(parsed) && parsed.length === columns.length) {
        const sanitized = sanitizeColumnWeights(parsed, fallbackWeights);
        migratedStorageKeyRef.current = storageKey;
        void setColumnLayoutAsync(storageKey, sanitized)
          .then(() => {
            try {
              localStorage.removeItem(storageKey);
            } catch {
              // ignore
            }
          })
          .catch(() => undefined);
      }
    } catch {
      // ignore
    }
  }, [
    storageKey,
    settingsConfirmed,
    settings.columnLayouts,
    columns.length,
    fallbackWeights,
    setColumnLayoutAsync,
  ]);

  // 滚动条尺寸观测
  useEffect(() => {
    const groupElement = groupElementRef.current;
    const viewport = scrollViewportRef.current;
    if (!viewport) return;
    const activeViewport = viewport;

    function updateScrollMetrics() {
      setScrollMetrics({
        clientWidth: activeViewport.clientWidth,
        scrollLeft: activeViewport.scrollLeft,
        scrollWidth: activeViewport.scrollWidth,
      });
    }

    if (typeof ResizeObserver === "undefined") {
      updateScrollMetrics();
      activeViewport.addEventListener("scroll", updateScrollMetrics, {
        passive: true,
      });
      return () => {
        activeViewport.removeEventListener("scroll", updateScrollMetrics);
      };
    }

    const resizeObserver = new ResizeObserver(updateScrollMetrics);
    if (groupElement) {
      resizeObserver.observe(groupElement);
    }
    resizeObserver.observe(activeViewport);
    activeViewport.addEventListener("scroll", updateScrollMetrics, {
      passive: true,
    });
    updateScrollMetrics();

    return () => {
      resizeObserver.disconnect();
      activeViewport.removeEventListener("scroll", updateScrollMetrics);
    };
  }, [childArray.length, minWidths, weights]);

  // 滚动条拖拽支持
  useEffect(() => {
    if (!scrollDragState) return;
    const activeDragState = scrollDragState;
    const viewport = scrollViewportRef.current;
    if (!viewport) return;
    const activeViewport = viewport;

    function handlePointerMove(event: PointerEvent) {
      const deltaRatio =
        activeDragState.trackTravelWidth > 0
          ? (event.clientX - activeDragState.startClientX) /
            activeDragState.trackTravelWidth
          : 0;
      activeViewport.scrollLeft = clamp(
        activeDragState.startScrollLeft +
          deltaRatio * activeDragState.maxScroll,
        0,
        activeDragState.maxScroll,
      );
    }

    function handlePointerUp() {
      setScrollDragState(null);
    }

    window.addEventListener("pointermove", handlePointerMove);
    window.addEventListener("pointerup", handlePointerUp, { once: true });

    return () => {
      window.removeEventListener("pointermove", handlePointerMove);
      window.removeEventListener("pointerup", handlePointerUp);
    };
  }, [scrollDragState]);

  const style: ResizableColumnsStyle = {
    "--resizable-columns-width": `${resizableCanvasWidth}px`,
    "--resizable-columns-min-width": `${totalMinimumWidth}px`,
  };

  const maxScroll = Math.max(
    0,
    scrollMetrics.scrollWidth - scrollMetrics.clientWidth,
  );
  const thumb = calculateScrollThumb(scrollMetrics);

  const handleLayoutChanged = (layout: Layout, meta: LayoutChangedMeta) => {
    const nextWeights = fromPanelLayout(layout, childArray.length);
    if (!nextWeights) return;

    setWeights(nextWeights);

    // 只有直接用户交互操作且配置了 storageKey 时才持久化
    if (!meta.isUserInteraction || !storageKey) return;

    if (arraysAlmostEqual(nextWeights, lastCommittedWeightsRef.current)) {
      return;
    }
    lastCommittedWeightsRef.current = nextWeights;
    void setColumnLayoutAsync(storageKey, nextWeights).catch(() => undefined);
  };

  function scrollColumns(delta: number) {
    scrollViewportRef.current?.scrollBy({ behavior: "smooth", left: delta });
  }

  function startScrollDrag(event: ReactPointerEvent<HTMLDivElement>) {
    const track = scrollTrackRef.current;
    const viewport = scrollViewportRef.current;
    if (!track || !viewport || maxScroll <= 0) return;

    event.preventDefault();
    event.stopPropagation();
    const trackTravelWidth =
      track.getBoundingClientRect().width * (1 - thumb.widthRatio);
    setScrollDragState({
      maxScroll,
      startClientX: event.clientX,
      startScrollLeft: viewport.scrollLeft,
      trackTravelWidth,
    });
  }

  function jumpScrollPosition(event: ReactPointerEvent<HTMLDivElement>) {
    const track = scrollTrackRef.current;
    const viewport = scrollViewportRef.current;
    if (!track || !viewport || maxScroll <= 0) return;

    const trackRect = track.getBoundingClientRect();
    const thumbWidth = trackRect.width * thumb.widthRatio;
    const trackTravelWidth = trackRect.width - thumbWidth;
    const nextThumbLeft = clamp(
      event.clientX - trackRect.left - thumbWidth / 2,
      0,
      trackTravelWidth,
    );
    viewport.scrollLeft =
      trackTravelWidth > 0 ? (nextThumbLeft / trackTravelWidth) * maxScroll : 0;
  }

  function handleScrollKeyDown(event: KeyboardEvent<HTMLDivElement>) {
    const viewport = scrollViewportRef.current;
    if (!viewport || maxScroll <= 0) return;

    if (event.key === "ArrowLeft") {
      event.preventDefault();
      scrollColumns(-SCROLL_BUTTON_STEP);
    } else if (event.key === "ArrowRight") {
      event.preventDefault();
      scrollColumns(SCROLL_BUTTON_STEP);
    } else if (event.key === "Home") {
      event.preventDefault();
      viewport.scrollTo({ behavior: "smooth", left: 0 });
    } else if (event.key === "End") {
      event.preventDefault();
      viewport.scrollTo({ behavior: "smooth", left: maxScroll });
    }
  }

  const totalWeight = weights.reduce((sum, w) => sum + w, 0) || 1;
  const elements: ReactNode[] = [];
  childArray.forEach((child, index) => {
    if (index > 0) {
      elements.push(
        <Separator
          aria-label={`${ariaLabel} ${index}`}
          className={cn(
            "relative z-10 w-3 -translate-x-1/2 cursor-col-resize touch-none outline-none",
            "aurora-resize-handle before:absolute before:inset-y-0 before:left-1/2 before:w-px before:-translate-x-1/2 before:bg-theme-card-border",
            "after:absolute after:left-1/2 after:top-1/2 after:h-10 after:w-1.5 after:-translate-x-1/2 after:-translate-y-1/2 after:rounded-full after:bg-theme-control-border after:opacity-0 after:transition-opacity",
            "hover:after:opacity-100 focus-visible:after:opacity-100 focus-visible:ring-2 focus-visible:ring-primary-strong/55",
            handleClassName,
          )}
          id={`separator-${index}`}
          key={`separator-${index}`}
        />,
      );
    }
    elements.push(
      <Panel
        defaultSize={`${(weights[index] / totalWeight) * 100}%`}
        id={`column-${index}`}
        key={`column-${index}`}
        minSize={minWidths[index]}
      >
        {child}
      </Panel>,
    );
  });

  return (
    <div
      className={cn(
        "relative isolate z-0 grid min-w-0 grid-rows-[minmax(0,1fr)_auto] overflow-visible",
        className,
      )}
    >
      <div className="min-h-0 min-w-0 overflow-hidden rounded-t-[inherit]">
        <div
          className="resizable-columns-viewport h-full min-h-0 min-w-0 overflow-x-auto overflow-y-hidden"
          ref={scrollViewportRef}
        >
          <Group
            className={cn(
              "relative h-full min-h-0 w-[max(100%,var(--resizable-columns-width))]",
              responsiveClassName,
            )}
            defaultLayout={toPanelLayout(weights)}
            elementRef={groupElementRef}
            groupRef={groupRef}
            id={storageKey ? `resizable-group-${storageKey}` : undefined}
            onLayoutChanged={handleLayoutChanged}
            orientation="horizontal"
            style={style}
          >
            {elements}
          </Group>
        </div>
      </div>

      <div
        className="sticky bottom-0 z-20 flex min-h-8 min-w-0 items-center gap-1 rounded-b-[inherit] border-t border-theme-card-border bg-theme-card-header/90 px-1.5 shadow-[0_-10px_24px_rgb(var(--theme-panel-shadow)/0.18)] backdrop-blur"
        data-resizable-columns-scroll-controls=""
      >
        <button
          aria-label={scrollLeftLabel}
          className="grid size-6 shrink-0 place-items-center rounded-xl text-on-surface-variant transition-[transform,background-color,color] duration-200 hover:-translate-y-px hover:bg-theme-control-hover hover:text-on-surface active:translate-y-0 disabled:cursor-default disabled:opacity-35"
          disabled={scrollMetrics.scrollLeft <= 0}
          onClick={() => scrollColumns(-SCROLL_BUTTON_STEP)}
          title={scrollLeftLabel}
          type="button"
        >
          <ChevronLeft size={15} />
        </button>
        <div
          className="relative h-5 min-w-0 flex-1 cursor-pointer"
          onPointerDown={jumpScrollPosition}
          ref={scrollTrackRef}
        >
          <div className="absolute inset-x-0 top-1/2 h-1.5 -translate-y-1/2 rounded-full bg-theme-control-border/75" />
          <div
            aria-disabled={maxScroll <= 0}
            aria-label={scrollBarLabel}
            aria-orientation="horizontal"
            aria-valuemax={Math.round(maxScroll)}
            aria-valuemin={0}
            aria-valuenow={Math.round(scrollMetrics.scrollLeft)}
            className={cn(
              "aurora-scroll-thumb absolute top-1/2 h-2.5 -translate-y-1/2 rounded-full border border-theme-nav-active-border/70 bg-theme-control-fg/75 shadow-[0_1px_3px_rgb(var(--theme-panel-shadow)/0.35)] outline-none",
              maxScroll > 0
                ? "cursor-grab hover:bg-primary active:cursor-grabbing focus-visible:ring-2 focus-visible:ring-primary-strong/55"
                : "cursor-default opacity-55",
            )}
            onKeyDown={handleScrollKeyDown}
            onPointerDown={startScrollDrag}
            role="scrollbar"
            style={{
              left: `${thumb.leftRatio * (1 - thumb.widthRatio) * 100}%`,
              width: `${thumb.widthRatio * 100}%`,
            }}
            tabIndex={0}
          />
        </div>
        <button
          aria-label={scrollRightLabel}
          className="grid size-6 shrink-0 place-items-center rounded-xl text-on-surface-variant transition-[transform,background-color,color] duration-200 hover:-translate-y-px hover:bg-theme-control-hover hover:text-on-surface active:translate-y-0 disabled:cursor-default disabled:opacity-35"
          disabled={scrollMetrics.scrollLeft >= maxScroll}
          onClick={() => scrollColumns(SCROLL_BUTTON_STEP)}
          title={scrollRightLabel}
          type="button"
        >
          <ChevronRight size={15} />
        </button>
      </div>
    </div>
  );
}

export function resolveColumnMinWidths(
  minimumWidth: number,
  columns: ResizableColumnConfig[],
) {
  return columns.map((column) =>
    Math.round(minimumWidth * (column.minWidthScale ?? 1)),
  );
}

export function calculateScrollThumb({
  clientWidth,
  scrollLeft,
  scrollWidth,
}: ScrollMetrics) {
  if (clientWidth <= 0 || scrollWidth <= clientWidth) {
    return {
      leftRatio: 0,
      widthRatio: 1,
    };
  }

  const maxScroll = scrollWidth - clientWidth;
  return {
    leftRatio: clamp(scrollLeft / maxScroll, 0, 1),
    widthRatio: clamp(clientWidth / scrollWidth, 0, 1),
  };
}

function readStoredColumnWeights(
  storageKey: string | undefined,
  fallbackWeights: readonly number[],
): number[] {
  if (!storageKey || typeof localStorage === "undefined") {
    return [...fallbackWeights];
  }

  try {
    const storedValue = localStorage.getItem(storageKey);
    if (!storedValue) return [...fallbackWeights];
    const parsedValue = JSON.parse(storedValue);
    return sanitizeColumnWeights(parsedValue, fallbackWeights);
  } catch {
    return [...fallbackWeights];
  }
}

function clamp(value: number, min: number, max: number) {
  return Math.min(Math.max(value, min), max);
}

function arraysAlmostEqual(left: readonly number[], right: readonly number[]) {
  if (left.length !== right.length) return false;
  return left.every((value, index) => Math.abs(value - right[index]) < 0.001);
}
