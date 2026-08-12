import clsx from "clsx";
import { useCallback, useLayoutEffect, useRef, useState, type CSSProperties, type ReactNode } from "react";
import { useI18n } from "../../../i18n/I18nProvider";
import { subNavLabel } from "../../../i18n/navigation";
import type { SubNavItem } from "../../../router/types";

export function SubNavigation({
  activeId,
  actions,
  items,
  onPrefetch,
  onSelect,
}: {
  activeId: string;
  actions?: ReactNode;
  items: SubNavItem[];
  onPrefetch?: (id: string) => void;
  onSelect?: (item: SubNavItem) => void;
}) {
  const { locale, t } = useI18n();
  const enabledItems = items.filter((item) => item.enabled);
  const tabRefs = useRef<Record<string, HTMLButtonElement | null>>({});
  const tabsViewportRef = useRef<HTMLDivElement | null>(null);
  const [indicator, setIndicator] = useState({ left: 0, width: 0, opacity: 0 });

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
      left: activeRect.left - viewportRect.left + viewport.scrollLeft,
      opacity: 1,
      width: activeRect.width,
    });
  }, [activeId]);

  useLayoutEffect(() => {
    const frame = window.requestAnimationFrame(updateIndicator);
    return () => window.cancelAnimationFrame(frame);
  }, [enabledItems.length, locale, updateIndicator]);

  useLayoutEffect(() => {
    const viewport = tabsViewportRef.current;
    if (!viewport) return;

    window.addEventListener("resize", updateIndicator);
    const observer = typeof ResizeObserver === "undefined" ? null : new ResizeObserver(updateIndicator);
    observer?.observe(viewport);
    return () => {
      window.removeEventListener("resize", updateIndicator);
      observer?.disconnect();
    };
  }, [updateIndicator]);

  const indicatorStyle = {
    left: indicator.left,
    opacity: indicator.opacity,
    width: indicator.width,
  } satisfies CSSProperties;

  return (
    <section
      className="aurora-subnav sticky top-[var(--app-window-titlebar-height)] z-10 flex shrink-0 items-center gap-3 border-b border-theme-card-border/45 px-[var(--app-page-x)] py-[var(--app-subnav-y)] backdrop-blur-xl"
      aria-label={t("nav.aria.subNav")}
    >
      <div className="aurora-pill-tabs-viewport flex min-w-0 flex-1 gap-1.5 overflow-x-auto" ref={tabsViewportRef}>
        <div className="relative flex min-w-max gap-1.5">
          <span aria-hidden="true" className="aurora-pill-indicator" style={indicatorStyle} />
          {enabledItems.map((item) => (
            <button
              className={clsx(
                "aurora-pill-tab relative h-8 whitespace-nowrap rounded-full border border-transparent px-4 text-body-sm font-medium text-on-surface-variant transition-colors hover:bg-theme-nav-hover/70 hover:text-on-surface",
                item.id === activeId &&
                  "aurora-pill-tab-active border-theme-nav-active-border/35 text-theme-nav-active-fg shadow-[inset_0_-2px_0_rgb(var(--theme-nav-indicator)/0.52)]",
              )}
              aria-current={item.id === activeId ? "page" : undefined}
              key={item.id}
              onClick={() => onSelect?.(item)}
              onFocus={() => onPrefetch?.(item.id)}
              onPointerEnter={() => onPrefetch?.(item.id)}
              ref={(element) => {
                tabRefs.current[item.id] = element;
              }}
              type="button"
            >
              {subNavLabel(item, t, locale)}
            </button>
          ))}
        </div>
      </div>
      {actions ? <div className="ml-auto flex shrink-0 items-center gap-2">{actions}</div> : null}
    </section>
  );
}
