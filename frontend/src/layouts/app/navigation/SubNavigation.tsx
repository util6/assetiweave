import clsx from "clsx";
import { useI18n } from "../../../i18n/I18nProvider";
import { subNavLabel } from "../../../i18n/navigation";
import type { SubNavItem } from "../../../router/types";

export function SubNavigation({
  activeId,
  items,
  onSelect,
}: {
  activeId: string;
  items: SubNavItem[];
  onSelect?: (item: SubNavItem) => void;
}) {
  const { locale, t } = useI18n();

  return (
    <section
      className="sticky top-0 z-10 flex shrink-0 gap-1.5 overflow-x-auto border-b border-theme-card-border bg-theme-subnav/88 px-[var(--app-page-x)] py-[var(--app-subnav-y)] backdrop-blur"
      aria-label={t("nav.aria.subNav")}
    >
      {items
        .filter((item) => item.enabled)
        .map((item) => (
          <button
            className={clsx(
              "relative h-8 whitespace-nowrap rounded-lg border border-transparent px-4 text-body-sm font-medium text-on-surface-variant transition-colors hover:bg-theme-nav-hover/70 hover:text-on-surface",
              item.id === activeId &&
                "border-theme-nav-active-border/35 bg-theme-nav-active text-theme-nav-active-fg shadow-[inset_0_-2px_0_rgb(var(--theme-nav-indicator)/0.52)]",
            )}
            key={item.id}
            onClick={() => onSelect?.(item)}
            type="button"
          >
            {subNavLabel(item, t, locale)}
          </button>
        ))}
    </section>
  );
}
