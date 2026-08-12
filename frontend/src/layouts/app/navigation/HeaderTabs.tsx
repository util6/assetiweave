import clsx from "clsx";
import { useI18n } from "../../../i18n/I18nProvider";
import { headerTabLabel } from "../../../i18n/navigation";
import type { HeaderTabItem } from "../../../router/types";

export function HeaderTabs({
  activeId,
  onSelect,
  tabs,
}: {
  activeId: string;
  onSelect: (tab: HeaderTabItem) => void;
  tabs: HeaderTabItem[];
}) {
  const { locale, t } = useI18n();

  return (
    <div
      className="flex h-11 max-w-full min-w-0 items-center gap-1 overflow-x-auto rounded-2xl border border-theme-control-border/70 bg-theme-control/68 p-1 shadow-[var(--theme-shadow-control-inset)] backdrop-blur-md"
      role="tablist"
      aria-label={t("nav.aria.assetTypes")}
    >
      {tabs
        .filter((tab) => tab.enabled)
        .map((tab) => (
          <button
            className={clsx(
              "h-8 min-w-24 whitespace-nowrap rounded-full px-5 text-center text-label-caps text-on-surface-variant transition-colors hover:bg-theme-control-hover hover:text-on-surface",
              tab.id === activeId && "bg-theme-nav-active text-theme-nav-active-fg shadow-[0_10px_24px_rgb(var(--theme-panel-shadow)/0.22),inset_0_1px_0_rgb(var(--theme-inset-highlight)/0.18)]",
            )}
            key={tab.id}
            onClick={() => onSelect(tab)}
            role="tab"
            aria-selected={tab.id === activeId}
          >
            {headerTabLabel(tab, t, locale)}
          </button>
        ))}
    </div>
  );
}
