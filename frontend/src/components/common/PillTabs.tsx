import clsx from "clsx";
import {
  useCallback,
  useLayoutEffect,
  useRef,
  useState,
  type CSSProperties,
  type ReactNode,
} from "react";

export interface PillTabItem<T extends string = string> {
  id: T;
  label: string;
  count?: number;
  icon?: ReactNode;
  disabled?: boolean;
}

export interface PillTabsProps<T extends string = string> {
  activeId: T;
  ariaLabel?: string;
  className?: string;
  containerClassName?: string;
  fullWidth?: boolean;
  items: PillTabItem<T>[];
  onSelect: (id: T, item: PillTabItem<T>) => void;
  size?: "sm" | "md";
}

export function PillTabs<T extends string = string>({
  activeId,
  ariaLabel,
  className,
  containerClassName,
  fullWidth = false,
  items,
  onSelect,
  size = "md",
}: PillTabsProps<T>) {
  const tabRefs = useRef<Record<string, HTMLButtonElement | null>>({});
  const tabsViewportRef = useRef<HTMLDivElement | null>(null);
  const [indicator, setIndicator] = useState({
    height: 0,
    left: 0,
    opacity: 0,
    top: 0,
    width: 0,
  });

  const updateIndicator = useCallback(() => {
    const activeTab = tabRefs.current[activeId];
    const viewport = tabsViewportRef.current;
    if (!activeTab || !viewport) {
      setIndicator((current) => ({ ...current, opacity: 0 }));
      return;
    }

    const activeRect = activeTab.getBoundingClientRect();
    const viewportRect = viewport.getBoundingClientRect();
    setIndicator({
      height: activeRect.height,
      left: activeRect.left - viewportRect.left + viewport.scrollLeft,
      opacity: 1,
      top: activeRect.top - viewportRect.top + viewport.scrollTop,
      width: activeRect.width,
    });
  }, [activeId]);

  useLayoutEffect(() => {
    const frame = window.requestAnimationFrame(updateIndicator);
    return () => window.cancelAnimationFrame(frame);
  }, [items.length, updateIndicator]);

  useLayoutEffect(() => {
    const viewport = tabsViewportRef.current;
    if (!viewport) return;

    window.addEventListener("resize", updateIndicator);
    const observer =
      typeof ResizeObserver === "undefined"
        ? null
        : new ResizeObserver(updateIndicator);
    observer?.observe(viewport);
    return () => {
      window.removeEventListener("resize", updateIndicator);
      observer?.disconnect();
    };
  }, [updateIndicator]);

  const indicatorStyle = {
    height: indicator.height > 0 ? indicator.height : undefined,
    left: indicator.left,
    opacity: indicator.opacity,
    top: indicator.top > 0 ? indicator.top : undefined,
    width: indicator.width,
  } satisfies CSSProperties;

  const isSm = size === "sm";

  return (
    <div
      aria-label={ariaLabel}
      className={clsx(
        "aurora-pill-tabs-viewport relative flex min-w-0 max-w-full items-center overflow-x-auto overflow-y-hidden rounded-full border border-theme-control-border/70 bg-theme-control/55 p-1 shadow-[var(--theme-shadow-control-inset)] backdrop-blur-md",
        containerClassName,
      )}
      ref={tabsViewportRef}
    >
      <div
        className={clsx(
          "relative flex items-center gap-1",
          fullWidth ? "w-full" : "min-w-max",
          className,
        )}
      >
        <span
          aria-hidden="true"
          className="aurora-pill-indicator"
          style={indicatorStyle}
        />
        {items.map((item) => {
          const isActive = item.id === activeId;
          return (
            <button
              aria-current={isActive ? "page" : undefined}
              aria-pressed={isActive}
              className={clsx(
                "aurora-pill-tab relative z-[1] inline-flex items-center justify-center whitespace-nowrap rounded-full border border-transparent text-center font-medium transition-colors cursor-pointer",
                isSm ? "h-7 px-2.5 text-caption" : "h-8 px-3.5 text-body-sm",
                fullWidth && "flex-1",
                isActive
                  ? "aurora-pill-tab-active border-theme-nav-active-border/35 text-theme-nav-active-fg shadow-[inset_0_-2px_0_rgb(var(--theme-nav-indicator)/0.52)] font-semibold"
                  : "text-on-surface-variant hover:bg-theme-nav-hover/70 hover:text-on-surface",
              )}
              disabled={item.disabled}
              key={item.id}
              onClick={() => onSelect(item.id, item)}
              ref={(element) => {
                tabRefs.current[item.id] = element;
              }}
              type="button"
            >
              {item.icon && (
                <span className="mr-1.5 shrink-0">{item.icon}</span>
              )}
              <span>{item.label}</span>
              {typeof item.count === "number" && (
                <span className="ml-1.5 rounded-full bg-theme-control/80 px-1.5 py-0.2 text-[10px] font-bold text-on-surface-variant">
                  {item.count}
                </span>
              )}
            </button>
          );
        })}
      </div>
    </div>
  );
}
