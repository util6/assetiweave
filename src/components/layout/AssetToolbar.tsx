import {
  Filter,
  Grid3X3,
  List,
  Plus,
  RefreshCw,
  Search,
  Settings,
  SlidersHorizontal,
  Tag,
} from "lucide-react";
import { useI18n } from "../../i18n/I18nProvider";
import { IconButton } from "../common/IconButton";
import { ToolbarButton } from "../common/ToolbarButton";

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
}) {
  const { t } = useI18n();

  return (
    <section
      className="sticky top-[var(--app-toolbar-top)] z-10 flex items-center justify-between gap-4 border-b border-border bg-surface-low/75 px-[var(--app-page-x)] py-[var(--app-toolbar-y)] shadow-[0_12px_28px_rgba(2,8,23,0.22)] backdrop-blur max-[1160px]:flex-col max-[1160px]:items-stretch"
      aria-label={t("toolbar.aria.assetActions")}
    >
      <div className="flex min-w-0 flex-1 items-center gap-3 max-[1160px]:flex-wrap">
        <label className="flex h-9 w-64 items-center gap-2 rounded-xl border border-border bg-surface-high/90 px-3 text-outline shadow-[inset_0_1px_0_rgba(255,255,255,0.04)] transition-colors focus-within:border-primary/60 focus-within:text-primary max-[1160px]:w-72">
          <Search size={17} />
          <input
            className="min-w-0 flex-1 border-0 bg-transparent text-body-sm text-on-surface outline-none placeholder:text-outline"
            placeholder={t("toolbar.searchPlaceholder")}
            value={query}
            onChange={(event) => onQueryChange(event.target.value)}
          />
        </label>
        <div className="flex h-9 items-center gap-1 rounded-xl border border-border bg-surface-card/70 p-1">
          <button className="grid size-7 place-items-center rounded-lg bg-status-update text-white" aria-label={t("toolbar.view.list")} type="button">
            <List size={17} />
          </button>
          <IconButton label={t("toolbar.view.grid")} icon={<Grid3X3 size={17} />} compact />
        </div>
        <ToolbarButton icon={<Filter size={17} />} label={t("toolbar.filter.all", { count: assetCount })} />
        <ToolbarButton icon={<Tag size={17} />} label={t("toolbar.filter.tag")} />
        <ToolbarButton icon={<SlidersHorizontal size={17} />} label={t("toolbar.sort.createdAt")} />
        <ToolbarMetric label={t("metric.sources")} value={sourceCount} />
        <ToolbarMetric label={t("metric.supportedApps")} value={supportAppCount} />
      </div>

      <div className="flex shrink-0 items-center justify-end gap-2 max-[1160px]:justify-start max-[1160px]:flex-wrap">
        <button
          className="grid size-10 place-items-center rounded-xl bg-gradient-to-br from-status-update to-status-create/80 text-white shadow-glow transition-transform hover:-translate-y-0.5 active:scale-95 disabled:cursor-not-allowed disabled:opacity-55"
          aria-label={t("toolbar.createDeploymentPlan")}
          onClick={onCreatePlan}
          disabled={busy}
          type="button"
        >
          <Plus size={22} />
        </button>
        <span className="mx-1 h-6 w-px bg-border" />
        <IconButton label={t("toolbar.scanSources")} icon={<RefreshCw size={17} />} onClick={onScan} disabled={busy} />
        <IconButton label={t("toolbar.settings")} icon={<Settings size={17} />} onClick={onOpenSettings} />
      </div>
    </section>
  );
}

function ToolbarMetric({ label, value }: { label: string; value: number }) {
  return (
    <div className="inline-flex h-9 min-w-24 items-center justify-between gap-3 rounded-xl border border-border bg-surface-high/70 px-3 text-body-sm shadow-[inset_0_1px_0_rgba(255,255,255,0.04)]">
      <span className="whitespace-nowrap text-on-surface-variant">{label}</span>
      <strong className="font-mono text-code-md text-primary">{value}</strong>
    </div>
  );
}
