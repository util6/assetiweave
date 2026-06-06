import { Archive } from "lucide-react";
import { AppUpdateButton } from "../../app/updates/AppUpdateButton";
import { useI18n } from "../../i18n/I18nProvider";
import type { HeaderTabItem, NavigationModel } from "../../router/types";
import type { AppOverview } from "../../types";
import { LanguageSwitcher } from "./LanguageSwitcher";
import { HeaderTabs } from "./navigation/HeaderTabs";

export function AppHeader({
  navigationModel,
  onHeaderTabSelect,
  overview,
}: {
  navigationModel: NavigationModel;
  onHeaderTabSelect: (tab: HeaderTabItem) => void;
  overview: AppOverview | null;
}) {
  const { t } = useI18n();

  return (
    <header className="sticky top-0 z-20 grid h-[var(--app-header-height)] shrink-0 grid-cols-[minmax(0,1fr)_minmax(0,2fr)_minmax(0,1fr)] items-center gap-4 bg-theme-toolbar/78 px-[var(--app-page-x)] shadow-[0_10px_28px_rgb(var(--theme-panel-shadow)/0.12)] backdrop-blur">
      <div className="flex min-w-0 items-center gap-2.5 text-h2 font-bold text-primary">
        <Archive size={22} />
        <span className="truncate">{t("app.title")}</span>
      </div>
      <HeaderTabs activeId={navigationModel.activeHeaderTabId} onSelect={onHeaderTabSelect} tabs={navigationModel.headerTabs} />
      <div className="flex min-w-0 items-center justify-end gap-3">
        <AppUpdateButton />
        <LanguageSwitcher />
      </div>
    </header>
  );
}
