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
import { cn } from "../../lib/utils";

export interface ResizableColumnConfig {
  defaultWeight: number;
  minWidthScale?: number;
}

export interface ResizeColumnWeightsOptions {
  containerWidth: number;
  deltaPx: number;
  handleIndex: number;
  minWidths: number[];
  weights: number[];
}

export interface ResizeColumnDragWeightsOptions extends ResizeColumnWeightsOptions {
  committedWeights: number[];
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

interface ColumnDragState {
  committedWeights: number[];
  containerWidth: number;
  handleIndex: number;
  startClientX: number;
  startWeights: number[];
}

interface ScrollDragState {
  maxScroll: number;
  startClientX: number;
  startScrollLeft: number;
  trackTravelWidth: number;
}

const KEYBOARD_RESIZE_STEP = 32;
const KEYBOARD_RESIZE_STEP_LARGE = 80;
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
  const fallbackWeights = useMemo(() => columns.map((column) => column.defaultWeight), [columns]);
  const minWidths = useMemo(() => resolveColumnMinWidths(minimumWidth, columns), [columns, minimumWidth]);
  const totalMinimumWidth = minWidths.reduce((sum, width) => sum + width, 0);
  const resizableCanvasWidth = totalMinimumWidth + Math.round(minimumWidth * 0.5);
  const [weights, setWeights] = useState(() => readStoredColumnWeights(storageKey, fallbackWeights));
  const [columnDragState, setColumnDragState] = useState<ColumnDragState | null>(null);
  const [handlePositions, setHandlePositions] = useState<number[]>([]);
  const [scrollDragState, setScrollDragState] = useState<ScrollDragState | null>(null);
  const [scrollMetrics, setScrollMetrics] = useState<ScrollMetrics>(EMPTY_SCROLL_METRICS);
  const gridRef = useRef<HTMLDivElement>(null);
  const scrollTrackRef = useRef<HTMLDivElement>(null);
  const scrollViewportRef = useRef<HTMLDivElement>(null);
  const childArray = Children.toArray(children);

  useEffect(() => {
    setWeights((currentWeights) => sanitizeColumnWeights(currentWeights, fallbackWeights));
  }, [fallbackWeights]);

  useEffect(() => {
    if (!storageKey) return;
    writeStoredColumnWeights(storageKey, weights);
  }, [storageKey, weights]);

  useEffect(() => {
    const grid = gridRef.current;
    const viewport = scrollViewportRef.current;
    if (!grid || !viewport) return;
    const activeGrid = grid;
    const activeViewport = viewport;

    function updateScrollMetrics() {
      setScrollMetrics({
        clientWidth: activeViewport.clientWidth,
        scrollLeft: activeViewport.scrollLeft,
        scrollWidth: activeViewport.scrollWidth,
      });
      setHandlePositions((currentPositions) => {
        const nextPositions = readColumnBoundaryPositions(activeGrid, childArray.length);
        return arraysEqual(currentPositions, nextPositions) ? currentPositions : nextPositions;
      });
    }

    const resizeObserver = new ResizeObserver(updateScrollMetrics);
    resizeObserver.observe(activeGrid);
    resizeObserver.observe(activeViewport);
    activeViewport.addEventListener("scroll", updateScrollMetrics, { passive: true });
    updateScrollMetrics();

    return () => {
      resizeObserver.disconnect();
      activeViewport.removeEventListener("scroll", updateScrollMetrics);
    };
  }, [childArray.length, minWidths, weights]);

  useEffect(() => {
    if (!columnDragState) return;
    const activeDragState = columnDragState;

    function handlePointerMove(event: PointerEvent) {
      setWeights(
        resizeColumnDragWeights({
          committedWeights: activeDragState.committedWeights,
          containerWidth: activeDragState.containerWidth,
          deltaPx: event.clientX - activeDragState.startClientX,
          handleIndex: activeDragState.handleIndex,
          minWidths,
          weights: activeDragState.startWeights,
        }),
      );
    }

    function handlePointerUp() {
      setColumnDragState(null);
    }

    window.addEventListener("pointermove", handlePointerMove);
    window.addEventListener("pointerup", handlePointerUp, { once: true });

    return () => {
      window.removeEventListener("pointermove", handlePointerMove);
      window.removeEventListener("pointerup", handlePointerUp);
    };
  }, [columnDragState, minWidths]);

  useEffect(() => {
    if (!scrollDragState) return;
    const activeDragState = scrollDragState;
    const viewport = scrollViewportRef.current;
    if (!viewport) return;
    const activeViewport = viewport;

    function handlePointerMove(event: PointerEvent) {
      const deltaRatio =
        activeDragState.trackTravelWidth > 0
          ? (event.clientX - activeDragState.startClientX) / activeDragState.trackTravelWidth
          : 0;
      activeViewport.scrollLeft = clamp(
        activeDragState.startScrollLeft + deltaRatio * activeDragState.maxScroll,
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

  const columnTemplate = minWidths
    .map((width, index) => `minmax(${width}px, ${formatWeight(weights[index] ?? columns[index].defaultWeight)}fr)`)
    .join(" ");
  const boundaries = getColumnBoundaries(weights);
  const style: ResizableColumnsStyle = {
    "--resizable-columns-width": `${resizableCanvasWidth}px`,
    "--resizable-columns-min-width": `${totalMinimumWidth}px`,
    "--resizable-columns-template": columnTemplate,
  };

  boundaries.forEach((boundary, index) => {
    style[`--resizable-column-boundary-${index}`] = String(boundary);
  });

  const maxScroll = Math.max(0, scrollMetrics.scrollWidth - scrollMetrics.clientWidth);
  const thumb = calculateScrollThumb(scrollMetrics);

  function startResize(handleIndex: number, event: ReactPointerEvent<HTMLDivElement>) {
    const grid = gridRef.current;
    const containerWidth = grid?.getBoundingClientRect().width ?? 0;
    if (!grid || containerWidth <= 0) return;

    event.preventDefault();
    setColumnDragState({
      committedWeights: weights,
      containerWidth,
      handleIndex,
      startClientX: event.clientX,
      startWeights: readMeasuredColumnWeights(grid, childArray.length, weights),
    });
  }

  function resizeFromKeyboard(handleIndex: number, event: KeyboardEvent<HTMLDivElement>) {
    if (event.key !== "ArrowLeft" && event.key !== "ArrowRight") return;

    const grid = gridRef.current;
    const containerWidth = grid?.getBoundingClientRect().width ?? 0;
    if (!grid || containerWidth <= 0) return;

    event.preventDefault();
    const direction = event.key === "ArrowRight" ? 1 : -1;
    const step = event.shiftKey ? KEYBOARD_RESIZE_STEP_LARGE : KEYBOARD_RESIZE_STEP;

    setWeights((currentWeights) =>
      resizeColumnWeights({
        containerWidth,
        deltaPx: direction * step,
        handleIndex,
        minWidths,
        weights: readMeasuredColumnWeights(grid, childArray.length, currentWeights),
      }),
    );
  }

  function scrollColumns(delta: number) {
    scrollViewportRef.current?.scrollBy({ behavior: "smooth", left: delta });
  }

  function startScrollDrag(event: ReactPointerEvent<HTMLDivElement>) {
    const track = scrollTrackRef.current;
    const viewport = scrollViewportRef.current;
    if (!track || !viewport || maxScroll <= 0) return;

    event.preventDefault();
    event.stopPropagation();
    const trackTravelWidth = track.getBoundingClientRect().width * (1 - thumb.widthRatio);
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
    const nextThumbLeft = clamp(event.clientX - trackRect.left - thumbWidth / 2, 0, trackTravelWidth);
    viewport.scrollLeft = trackTravelWidth > 0 ? (nextThumbLeft / trackTravelWidth) * maxScroll : 0;
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

  return (
    <div className={cn("grid min-w-0 grid-rows-[minmax(0,1fr)_auto] overflow-visible", className, "overflow-visible")}>
      <div className="min-h-0 min-w-0 overflow-hidden rounded-t-[inherit]">
        <div className="resizable-columns-viewport min-h-0 min-w-0 overflow-x-auto overflow-y-hidden" ref={scrollViewportRef}>
          <div
            className={cn(
              "relative grid h-full min-h-0 w-[max(100%,var(--resizable-columns-width))] grid-cols-[var(--resizable-columns-template)]",
              responsiveClassName,
            )}
            ref={gridRef}
            style={style}
          >
            {childArray}
            {boundaries.map((boundary, index) => (
              <div
                aria-label={`${ariaLabel} ${index + 1}`}
                aria-orientation="vertical"
                className={cn(
                  "absolute inset-y-0 z-10 w-3 -translate-x-1/2 cursor-col-resize touch-none outline-none",
                  "before:absolute before:inset-y-0 before:left-1/2 before:w-px before:-translate-x-1/2 before:bg-theme-card-border",
                  "after:absolute after:left-1/2 after:top-1/2 after:h-10 after:w-1 after:-translate-x-1/2 after:-translate-y-1/2 after:rounded-full after:bg-theme-control-border after:opacity-0 after:transition-opacity",
                  "hover:after:opacity-100 focus-visible:after:opacity-100 focus-visible:ring-2 focus-visible:ring-primary-strong/55",
                  columnDragState?.handleIndex === index && "after:opacity-100",
                  handleClassName,
                )}
                key={index}
                onKeyDown={(event) => resizeFromKeyboard(index, event)}
                onPointerDown={(event) => startResize(index, event)}
                role="separator"
                style={{
                  left:
                    handlePositions[index] === undefined
                      ? `calc(var(--resizable-column-boundary-${index}) * 100%)`
                      : `${handlePositions[index]}px`,
                }}
                tabIndex={0}
              />
            ))}
          </div>
        </div>
      </div>

      <div
        className="sticky bottom-0 z-20 flex min-h-8 min-w-0 items-center gap-1 rounded-b-[inherit] border-t border-theme-card-border bg-theme-card-header/90 px-1.5 shadow-[0_-10px_24px_rgb(var(--theme-panel-shadow)/0.18)] backdrop-blur"
        data-resizable-columns-scroll-controls=""
      >
        <button
          aria-label={scrollLeftLabel}
          className="grid size-6 shrink-0 place-items-center rounded-md text-on-surface-variant transition-colors hover:bg-theme-control-hover hover:text-on-surface disabled:cursor-default disabled:opacity-35"
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
              "absolute top-1/2 h-2.5 -translate-y-1/2 rounded-full border border-theme-nav-active-border/70 bg-theme-control-fg/75 shadow-[0_1px_3px_rgb(var(--theme-panel-shadow)/0.35)] outline-none",
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
          className="grid size-6 shrink-0 place-items-center rounded-md text-on-surface-variant transition-colors hover:bg-theme-control-hover hover:text-on-surface disabled:cursor-default disabled:opacity-35"
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

export function resizeColumnWeights({
  containerWidth,
  deltaPx,
  handleIndex,
  minWidths,
  weights,
}: ResizeColumnWeightsOptions) {
  if (containerWidth <= 0 || handleIndex < 0 || handleIndex >= weights.length - 1) return weights;

  const totalWeight = weights.reduce((sum, weight) => sum + weight, 0);
  if (totalWeight <= 0) return weights;

  const leftWeight = weights[handleIndex];
  const rightWeight = weights[handleIndex + 1];
  const pairWeight = leftWeight + rightWeight;
  const pairWidth = (pairWeight / totalWeight) * containerWidth;
  const minLeft = minWidths[handleIndex] ?? 0;
  const minRight = minWidths[handleIndex + 1] ?? 0;
  const minPairWidth = minLeft + minRight;

  if (pairWidth < minPairWidth) return weights;

  const currentLeftWidth = (leftWeight / pairWeight) * pairWidth;
  const nextLeftWidth = clamp(currentLeftWidth + deltaPx, minLeft, pairWidth - minRight);
  const nextWeights = [...weights];

  nextWeights[handleIndex] = (nextLeftWidth / pairWidth) * pairWeight;
  nextWeights[handleIndex + 1] = ((pairWidth - nextLeftWidth) / pairWidth) * pairWeight;

  return nextWeights;
}

export function resizeColumnDragWeights({
  committedWeights,
  containerWidth,
  deltaPx,
  handleIndex,
  minWidths,
  weights,
}: ResizeColumnDragWeightsOptions) {
  const nextWeights = resizeColumnWeights({
    containerWidth,
    deltaPx,
    handleIndex,
    minWidths,
    weights,
  });

  return arraysAlmostEqual(nextWeights, weights) ? committedWeights : nextWeights;
}

export function sanitizeColumnWeights(weights: number[], fallbackWeights: number[]) {
  if (weights.length !== fallbackWeights.length) return fallbackWeights;
  if (weights.some((weight) => !Number.isFinite(weight) || weight <= 0)) return fallbackWeights;
  return scaleWeightsToTotal(weights, fallbackWeights.reduce((sum, weight) => sum + weight, 0));
}

export function getColumnBoundaries(weights: number[]) {
  const totalWeight = weights.reduce((sum, weight) => sum + weight, 0);
  if (totalWeight <= 0) {
    return weights.slice(0, -1).map((_, index) => (index + 1) / weights.length);
  }

  let runningWeight = 0;
  return weights.slice(0, -1).map((weight) => {
    runningWeight += weight;
    return runningWeight / totalWeight;
  });
}

export function resolveColumnMinWidths(minimumWidth: number, columns: ResizableColumnConfig[]) {
  return columns.map((column) => Math.round(minimumWidth * (column.minWidthScale ?? 1)));
}

export function calculateScrollThumb({ clientWidth, scrollLeft, scrollWidth }: ScrollMetrics) {
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

function readMeasuredColumnWeights(grid: HTMLDivElement, columnCount: number, fallbackWeights: number[]) {
  const widths = Array.from(grid.children)
    .slice(0, columnCount)
    .map((child) => child.getBoundingClientRect().width);
  const totalWidth = widths.reduce((sum, width) => sum + width, 0);

  if (widths.length !== fallbackWeights.length || totalWidth <= 0 || widths.some((width) => width <= 0)) {
    return fallbackWeights;
  }

  return scaleWeightsToTotal(widths, fallbackWeights.reduce((sum, weight) => sum + weight, 0));
}

function readColumnBoundaryPositions(grid: HTMLDivElement, columnCount: number) {
  return Array.from(grid.children)
    .slice(0, Math.max(0, columnCount - 1))
    .map((child) => {
      const element = child as HTMLElement;
      return element.offsetLeft + element.offsetWidth;
    });
}

function readStoredColumnWeights(storageKey: string | undefined, fallbackWeights: number[]) {
  if (!storageKey || typeof localStorage === "undefined") return fallbackWeights;

  try {
    const storedValue = localStorage.getItem(storageKey);
    if (!storedValue) return fallbackWeights;
    const parsedValue = JSON.parse(storedValue);
    if (!Array.isArray(parsedValue)) return fallbackWeights;
    return sanitizeColumnWeights(parsedValue, fallbackWeights);
  } catch {
    return fallbackWeights;
  }
}

function writeStoredColumnWeights(storageKey: string, weights: number[]) {
  if (typeof localStorage === "undefined") return;

  try {
    localStorage.setItem(storageKey, JSON.stringify(weights));
  } catch {
    // Persisted UI preference is best-effort only.
  }
}

function formatWeight(weight: number) {
  return Number.isFinite(weight) && weight > 0 ? Number(weight.toFixed(4)) : 1;
}

function clamp(value: number, min: number, max: number) {
  return Math.min(Math.max(value, min), max);
}

function arraysEqual(left: number[], right: number[]) {
  return left.length === right.length && left.every((value, index) => value === right[index]);
}

function arraysAlmostEqual(left: number[], right: number[]) {
  return left.length === right.length && left.every((value, index) => Math.abs(value - right[index]) < 0.001);
}

function scaleWeightsToTotal(weights: number[], targetTotal: number) {
  const currentTotal = weights.reduce((sum, weight) => sum + weight, 0);
  if (currentTotal <= 0 || targetTotal <= 0) return weights;
  const scale = targetTotal / currentTotal;
  return weights.map((weight) => weight * scale);
}
