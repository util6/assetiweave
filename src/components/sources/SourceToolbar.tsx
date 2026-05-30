import { Columns3, FolderPlus, LayoutList, RefreshCw, Settings } from "lucide-react";
import { useI18n } from "../../i18n/I18nProvider";
import {
  DataToolbar,
  ToolbarActionButton,
  ToolbarSearch,
  ToolbarSeparator,
  ToolbarViewToggle,
  type ToolbarViewMode,
} from "../common/DataToolbar";

export type SourceViewMode = Extract<ToolbarViewMode, "list" | "columns">;

export function SourceToolbar({
  busy,
  viewMode,
  query,
  onImport,
  onOpenSettings,
  onQueryChange,
  onScan,
  onViewModeChange,
}: {
  busy: boolean;
  viewMode: SourceViewMode;
  query: string;
  onImport: () => void;
  onOpenSettings: () => void;
  onQueryChange: (query: string) => void;
  onScan: () => void;
  onViewModeChange: (viewMode: SourceViewMode) => void;
}) {
  const { t } = useI18n();

  return (
    <DataToolbar
      actions={
        <>
          <ToolbarViewToggle
            ariaLabel={t("toolbar.view.aria")}
            onChange={onViewModeChange}
            options={[
              { icon: <LayoutList size={17} />, label: t("toolbar.view.list"), value: "list" },
              { icon: <Columns3 size={17} />, label: t("toolbar.view.columns"), value: "columns" },
            ]}
            value={viewMode}
          />
          <ToolbarSeparator />
          <ToolbarActionButton
            disabled={busy}
            icon={<FolderPlus size={17} />}
            label={t("source.toolbar.add")}
            onClick={onImport}
            primary
            text={t("source.toolbar.add")}
          />
          <ToolbarSeparator />
          <ToolbarActionButton disabled={busy} icon={<RefreshCw size={17} />} label={t("source.toolbar.scanAll")} onClick={onScan} />
          <ToolbarActionButton icon={<Settings size={17} />} label={t("toolbar.settings")} onClick={onOpenSettings} />
        </>
      }
      ariaLabel={t("source.page.title")}
      leading={
        <ToolbarSearch
          className="flex-1"
          onChange={onQueryChange}
          placeholder={t("source.toolbar.searchPlaceholder")}
          value={query}
        />
      }
    />
  );
}
