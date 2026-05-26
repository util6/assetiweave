import clsx from "clsx";
import { useI18n } from "../../i18n/I18nProvider";
import { headerTabLabel } from "../../i18n/navigation";
import type { HeaderTabItem } from "../../navigation/types";

export function HeaderTabs({ activeId, tabs }: { activeId: string; tabs: HeaderTabItem[] }) {
  const { t } = useI18n();

  return (
    <div
      className="flex gap-0.5 rounded-full border border-border bg-surface-low/90 p-1"
      role="tablist"
      aria-label={t("nav.aria.assetTypes")}
    >
      {tabs
        .filter((tab) => tab.enabled)
        .map((tab) => (
          <button
            className={clsx(
              "min-w-24 rounded-full px-5 py-2 text-label-caps text-on-surface-variant transition-colors hover:text-on-surface",
              tab.id === activeId && "bg-surface-highest text-primary shadow-lg",
            )}
            key={tab.id}
            role="tab"
            aria-selected={tab.id === activeId}
          >
            {headerTabLabel(tab, t)}
          </button>
        ))}
    </div>
  );
}
