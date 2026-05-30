import { Archive } from "lucide-react";
import { translateScanStatus } from "../../i18n/domain";
import { useI18n } from "../../i18n/I18nProvider";
import type { NavigationModel } from "../../router/types";
import type { AppOverview } from "../../types";
import { LanguageSwitcher } from "./LanguageSwitcher";
import { HeaderTabs } from "./navigation/HeaderTabs";

export function AppHeader({
  navigationModel,
  overview,
}: {
  navigationModel: NavigationModel;
  overview: AppOverview | null;
}) {
  const { t } = useI18n();

  return (
    <header className="sticky top-0 z-20 grid h-[var(--app-header-height)] shrink-0 grid-cols-[minmax(180px,1fr)_auto_minmax(360px,1fr)] items-center gap-4 px-[var(--app-page-x)] backdrop-blur">
      <div className="flex items-center gap-2.5 text-h2 font-bold text-status-update">
        <Archive size={22} />
        <span>{t("app.title")}</span>
      </div>
      <HeaderTabs activeId={navigationModel.activeHeaderTabId} tabs={navigationModel.headerTabs} />
      <div className="flex min-w-0 items-center justify-end gap-3">
        <div className="min-w-0 max-w-[360px] overflow-hidden text-ellipsis whitespace-nowrap text-body-sm text-outline">
          {translateScanStatus(overview?.last_scan_status, t)}
        </div>
        <LanguageSwitcher />
      </div>
    </header>
  );
}
