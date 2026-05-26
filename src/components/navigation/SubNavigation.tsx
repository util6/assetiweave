import clsx from "clsx";
import { useI18n } from "../../i18n/I18nProvider";
import { subNavLabel } from "../../i18n/navigation";
import type { SubNavItem } from "../../navigation/types";

export function SubNavigation({ activeId, items }: { activeId: string; items: SubNavItem[] }) {
  const { t } = useI18n();

  return (
    <section
      className="sticky top-16 z-10 flex shrink-0 gap-3 border-y border-border bg-surface-lowest/70 px-8 py-3 backdrop-blur"
      aria-label={t("nav.aria.subNav")}
    >
      {items
        .filter((item) => item.enabled)
        .map((item) => (
          <button
            className={clsx(
              "rounded-full border border-transparent px-4 py-1.5 text-body-sm text-on-surface-variant transition-colors hover:bg-surface-high hover:text-on-surface",
              item.id === activeId && "border-primary-strong/30 bg-surface-high text-primary",
            )}
            key={item.id}
          >
            {subNavLabel(item, t)}
          </button>
        ))}
    </section>
  );
}
