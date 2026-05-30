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
  const { t } = useI18n();

  return (
    <section
      className="sticky top-[var(--app-header-height)] z-10 flex shrink-0 gap-1.5 border-y border-border bg-surface-lowest/80 px-[var(--app-page-x)] py-[var(--app-subnav-y)] backdrop-blur"
      aria-label={t("nav.aria.subNav")}
    >
      {items
        .filter((item) => item.enabled)
        .map((item) => (
          <button
            className={clsx(
              "relative h-8 whitespace-nowrap rounded-lg border border-transparent px-4 text-body-sm font-medium text-on-surface-variant transition-colors hover:bg-surface-high/70 hover:text-on-surface",
              item.id === activeId && "border-primary-strong/25 bg-surface-high text-primary shadow-[inset_0_-2px_0_rgba(173,198,255,0.48)]",
            )}
            key={item.id}
            onClick={() => onSelect?.(item)}
            type="button"
          >
            {subNavLabel(item, t)}
          </button>
        ))}
    </section>
  );
}
