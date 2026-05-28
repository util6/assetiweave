import clsx from "clsx";
import { useI18n } from "../../i18n/I18nProvider";
import { headerTabLabel } from "../../i18n/navigation";
import type { HeaderTabItem } from "../../navigation/types";

export function HeaderTabs({ activeId, tabs }: { activeId: string; tabs: HeaderTabItem[] }) {
  const { t } = useI18n();

  return (
    <div
      className="flex h-11 items-center gap-1 rounded-xl border border-border bg-surface-low/90 p-1 shadow-[inset_0_1px_0_rgba(255,255,255,0.04)]"
      role="tablist"
      aria-label={t("nav.aria.assetTypes")}
    >
      {tabs
        .filter((tab) => tab.enabled)
        .map((tab) => (
          <button
            className={clsx(
              "h-8 min-w-24 whitespace-nowrap rounded-lg px-5 text-center text-label-caps text-on-surface-variant transition-colors hover:bg-surface-high hover:text-on-surface",
              tab.id === activeId && "bg-surface-highest text-primary shadow-[0_10px_24px_rgba(2,8,23,0.28)]",
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
