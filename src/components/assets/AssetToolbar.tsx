import {
  Filter,
  Grid3X3,
  LayoutList,
  Plus,
  RefreshCw,
  Settings,
  SlidersHorizontal,
  Tag,
} from "lucide-react";
import { useI18n } from "../../i18n/I18nProvider";
import {
  DataToolbar,
  ToolbarActionButton,
  ToolbarMetric,
  ToolbarSearch,
  ToolbarSeparator,
  ToolbarTextButton,
  ToolbarViewToggle,
  type ToolbarViewMode,
} from "../common/DataToolbar";

export type AssetViewMode = Extract<ToolbarViewMode, "list" | "grid">;

export function AssetToolbar({
  query,
  assetCount,
  sourceCount,
  supportAppCount,
  busy,
  onQueryChange,
  onScan,
  onCreatePlan,
  onOpenSettings,
  onViewModeChange,
  viewMode,
}: {
  query: string;
  assetCount: number;
  sourceCount: number;
  supportAppCount: number;
  busy: boolean;
  onQueryChange: (query: string) => void;
  onScan: () => void;
  onCreatePlan: () => void;
  onOpenSettings?: () => void;
  onViewModeChange: (viewMode: AssetViewMode) => void;
  viewMode: AssetViewMode;
}) {
  const { t } = useI18n();

  return (
    <DataToolbar
      actions={
        <>
          <ToolbarActionButton
            disabled={busy}
            icon={<Plus size={22} />}
            label={t("toolbar.createDeploymentPlan")}
            onClick={onCreatePlan}
            primary
          />
          <ToolbarSeparator />
          <ToolbarActionButton disabled={busy} icon={<RefreshCw size={17} />} label={t("toolbar.scanSources")} onClick={onScan} />
          <ToolbarActionButton icon={<Settings size={17} />} label={t("toolbar.settings")} onClick={onOpenSettings} />
        </>
      }
      ariaLabel={t("toolbar.aria.assetActions")}
      leading={
        <>
          <ToolbarSearch
            className="w-64 max-[1160px]:w-72"
            onChange={onQueryChange}
            placeholder={t("toolbar.searchPlaceholder")}
            value={query}
          />
          <ToolbarViewToggle
            ariaLabel={t("toolbar.view.aria")}
            onChange={onViewModeChange}
            options={[
              { icon: <LayoutList size={17} />, label: t("toolbar.view.list"), value: "list" },
              { icon: <Grid3X3 size={17} />, label: t("toolbar.view.grid"), value: "grid" },
            ]}
            value={viewMode}
          />
          <ToolbarTextButton icon={<Filter size={17} />} label={t("toolbar.filter.all", { count: assetCount })} />
          <ToolbarTextButton icon={<Tag size={17} />} label={t("toolbar.filter.tag")} />
          <ToolbarTextButton icon={<SlidersHorizontal size={17} />} label={t("toolbar.sort.createdAt")} />
          <ToolbarMetric label={t("metric.sources")} value={sourceCount} />
          <ToolbarMetric label={t("metric.supportedApps")} value={supportAppCount} />
        </>
      }
      sticky
    />
  );
}
