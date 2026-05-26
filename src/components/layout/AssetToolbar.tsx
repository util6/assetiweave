import {
  Download,
  Eye,
  Filter,
  Folder,
  Grid3X3,
  List,
  Menu,
  Plus,
  RefreshCw,
  Search,
  Settings,
  SlidersHorizontal,
  Tag,
  Upload,
} from "lucide-react";
import { useI18n } from "../../i18n/I18nProvider";
import { IconButton } from "../common/IconButton";
import { ToolbarButton } from "../common/ToolbarButton";

export function AssetToolbar({
  query,
  assetCount,
  busy,
  hasPlan,
  onQueryChange,
  onScan,
  onCreatePlan,
  onExecutePlan,
}: {
  query: string;
  assetCount: number;
  busy: boolean;
  hasPlan: boolean;
  onQueryChange: (query: string) => void;
  onScan: () => void;
  onCreatePlan: () => void;
  onExecutePlan: () => void;
}) {
  const { t } = useI18n();

  return (
    <section
      className="sticky top-[113px] z-10 flex justify-between gap-4 border-y border-border bg-surface-low/50 px-8 py-4 backdrop-blur max-[1160px]:flex-col"
      aria-label={t("toolbar.aria.assetActions")}
    >
      <div className="flex items-center gap-3 max-[1160px]:flex-wrap">
        <label className="flex h-9 w-56 items-center gap-2 rounded-xl border border-border bg-surface-high px-3 text-outline focus-within:border-primary/50">
          <Search size={17} />
          <input
            className="min-w-0 flex-1 border-0 bg-transparent text-body-sm text-on-surface outline-none placeholder:text-outline"
            placeholder={t("toolbar.searchPlaceholder")}
            value={query}
            onChange={(event) => onQueryChange(event.target.value)}
          />
        </label>
        <div className="flex h-9 items-center gap-1 rounded-xl border border-border bg-surface-high p-1">
          <IconButton label={t("toolbar.view.compact")} icon={<Menu size={17} />} compact />
          <button className="grid size-7 place-items-center rounded-lg bg-status-update text-white" aria-label={t("toolbar.view.list")} type="button">
            <List size={17} />
          </button>
          <IconButton label={t("toolbar.view.grid")} icon={<Grid3X3 size={17} />} compact />
        </div>
        <ToolbarButton icon={<Filter size={17} />} label={t("toolbar.filter.all", { count: assetCount })} />
        <ToolbarButton icon={<Tag size={17} />} label={t("toolbar.filter.tag")} />
        <ToolbarButton icon={<SlidersHorizontal size={17} />} label={t("toolbar.sort.createdAt")} />
        <IconButton label={t("toolbar.export")} icon={<Download size={17} />} />
      </div>

      <div className="flex items-center gap-2 max-[1160px]:flex-wrap">
        <button
          className="grid size-10 place-items-center rounded-xl bg-gradient-to-br from-status-update to-status-create/70 text-white shadow-glow transition-transform hover:-translate-y-0.5 active:scale-95"
          aria-label={t("toolbar.createDeploymentPlan")}
          onClick={onCreatePlan}
          disabled={busy}
          type="button"
        >
          <Plus size={22} />
        </button>
        <span className="mx-1 h-6 w-px bg-border" />
        <IconButton label={t("toolbar.scanSources")} icon={<RefreshCw size={17} />} onClick={onScan} disabled={busy} />
        <IconButton label={t("toolbar.generatePlan")} icon={<Eye size={17} />} onClick={onCreatePlan} disabled={busy} />
        <IconButton label={t("toolbar.executePlan")} icon={<Upload size={17} />} onClick={onExecutePlan} disabled={busy || !hasPlan} />
        <IconButton label={t("toolbar.openFolder")} icon={<Folder size={17} />} />
        <IconButton label={t("toolbar.settings")} icon={<Settings size={17} />} />
      </div>
    </section>
  );
}
