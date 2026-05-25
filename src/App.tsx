import { useEffect, useMemo, useState } from "react";
import clsx from "clsx";
import {
  Archive,
  Download,
  Eye,
  Filter,
  Folder,
  Grid3X3,
  List,
  Menu,
  Pencil,
  Plus,
  RefreshCw,
  Search,
  Settings,
  SlidersHorizontal,
  Tag,
  Trash2,
  Upload,
} from "lucide-react";
import { HeaderTabs } from "./components/navigation/HeaderTabs";
import { SideRail } from "./components/navigation/SideRail";
import { SubNavigation } from "./components/navigation/SubNavigation";
import { navigationModel as fallbackNavigationModel } from "./navigation/menu";
import type { NavigationModel } from "./navigation/types";
import { createPlan, executePlan, getNavigationModel, getOverview, listAssets, revealPath, scanSources } from "./services/catalog";
import type { AppOverview, Asset, AssetKind, DeploymentPlan, ExecutionResult } from "./types";

const kindLabel: Record<AssetKind, string> = {
  prompt: "Prompt",
  rule: "Rule",
  memory: "Memory",
  skill: "Skill",
  mcp: "MCP",
  agent: "Agent",
  command: "Command",
  workflow: "Workflow",
  profile: "Profile",
  custom: "Custom",
  unclassified: "Unclassified",
};

export function App() {
  const [assets, setAssets] = useState<Asset[]>([]);
  const [overview, setOverview] = useState<AppOverview | null>(null);
  const [plan, setPlan] = useState<DeploymentPlan | null>(null);
  const [executionResult, setExecutionResult] = useState<ExecutionResult | null>(null);
  const [navigationModel, setNavigationModel] = useState<NavigationModel>(fallbackNavigationModel);
  const [busy, setBusy] = useState(false);
  const [expandedIds, setExpandedIds] = useState<Set<string>>(new Set());
  const [query, setQuery] = useState("");
  const activeSubNavItems = navigationModel.subNavItems[navigationModel.activeHeaderTabId] ?? [];

  useEffect(() => {
    void Promise.all([listAssets(), getOverview(), getNavigationModel()]).then(([assetList, appOverview, appNavigationModel]) => {
      setAssets(assetList);
      setOverview(appOverview);
      setNavigationModel(appNavigationModel);
    });
  }, []);

  const filteredAssets = useMemo(() => {
    const normalized = query.trim().toLowerCase();
    if (!normalized) return assets;

    return assets.filter((asset) =>
      [asset.name, asset.kind, asset.format, asset.relative_path, asset.absolute_path, asset.description ?? ""]
        .join(" ")
        .toLowerCase()
        .includes(normalized),
    );
  }, [assets, query]);

  function toggleAsset(id: string) {
    setExpandedIds((current) => {
      const next = new Set(current);
      if (next.has(id)) {
        next.delete(id);
      } else {
        next.add(id);
      }
      return next;
    });
  }

  async function refreshOverview(nextAssets?: Asset[]) {
    const [assetList, appOverview] = await Promise.all([
      nextAssets ? Promise.resolve(nextAssets) : listAssets(),
      getOverview(),
    ]);
    setAssets(assetList);
    setOverview(appOverview);
  }

  async function handleScan() {
    setBusy(true);
    try {
      const scannedAssets = await scanSources();
      await refreshOverview(scannedAssets);
      setPlan(null);
      setExecutionResult(null);
    } finally {
      setBusy(false);
    }
  }

  async function handleCreatePlan() {
    setBusy(true);
    try {
      setPlan(await createPlan());
      setExecutionResult(null);
    } finally {
      setBusy(false);
    }
  }

  async function handleExecutePlan() {
    if (!plan) return;
    setBusy(true);
    try {
      setExecutionResult(await executePlan(plan));
      await refreshOverview();
    } finally {
      setBusy(false);
    }
  }

  return (
    <div className="grid-texture flex min-h-screen bg-background text-on-surface">
      <SideRail activeId={navigationModel.activeRailId} items={navigationModel.railItems} />

      <main className="ml-sidebar-width flex min-h-screen w-[calc(100%-64px)] flex-1 flex-col">
        <header className="sticky top-0 z-20 flex h-16 shrink-0 items-center px-8 backdrop-blur">
          <div className="flex items-center gap-2.5 text-h2 font-bold text-status-update">
            <Archive size={22} />
            <span>资产目录</span>
          </div>
          <HeaderTabs activeId={navigationModel.activeHeaderTabId} tabs={navigationModel.headerTabs} />
          <div className="ml-auto max-w-[300px] overflow-hidden text-ellipsis whitespace-nowrap text-body-sm text-outline">
            {overview?.last_scan_status ?? "加载中..."}
          </div>
        </header>

        <SubNavigation activeId={navigationModel.activeSubNavId} items={activeSubNavItems} />

        <section
          className="sticky top-[113px] z-10 flex justify-between gap-4 border-b border-border bg-surface-low/50 px-8 py-4 backdrop-blur max-[1160px]:flex-col"
          aria-label="资产操作栏"
        >
          <div className="flex items-center gap-3 max-[1160px]:flex-wrap">
            <label className="flex h-9 w-56 items-center gap-2 rounded-xl border border-border bg-surface-high px-3 text-outline focus-within:border-primary/50">
              <Search size={17} />
              <input
                className="min-w-0 flex-1 border-0 bg-transparent text-body-sm text-on-surface outline-none placeholder:text-outline"
                placeholder="搜索资产..."
                value={query}
                onChange={(event) => setQuery(event.target.value)}
              />
            </label>
            <div className="flex h-9 items-center gap-1 rounded-xl border border-border bg-surface-high p-1">
              <IconButton label="紧凑视图" icon={<Menu size={17} />} compact />
              <button className="grid size-7 place-items-center rounded-lg bg-status-update text-white" aria-label="列表视图">
                <List size={17} />
              </button>
              <IconButton label="网格视图" icon={<Grid3X3 size={17} />} compact />
            </div>
            <ToolbarButton icon={<Filter size={17} />} label={`全部 (${overview?.asset_count ?? assets.length})`} />
            <ToolbarButton icon={<Tag size={17} />} label="标签筛选" />
            <ToolbarButton icon={<SlidersHorizontal size={17} />} label="按创建时间" />
            <IconButton label="导出" icon={<Download size={17} />} />
          </div>

          <div className="flex items-center gap-2 max-[1160px]:flex-wrap">
            <button
              className="grid size-10 place-items-center rounded-xl bg-gradient-to-br from-status-update to-status-create/70 text-white shadow-glow transition-transform hover:-translate-y-0.5 active:scale-95"
              aria-label="生成部署计划"
              onClick={handleCreatePlan}
              disabled={busy}
            >
              <Plus size={22} />
            </button>
            <span className="mx-1 h-6 w-px bg-border" />
            <IconButton label="扫描资产源" icon={<RefreshCw size={17} />} onClick={handleScan} disabled={busy} />
            <IconButton label="生成计划" icon={<Eye size={17} />} onClick={handleCreatePlan} disabled={busy} />
            <IconButton label="执行当前计划" icon={<Upload size={17} />} onClick={handleExecutePlan} disabled={busy || !plan} />
            {[Folder, Settings].map((Icon) => (
              <IconButton label={Icon.displayName ?? Icon.name} icon={<Icon size={17} />} key={Icon.name} />
            ))}
          </div>
        </section>

        <section className="flex flex-1 flex-col gap-4 px-8 py-6">
          <div className="grid grid-cols-4 gap-3">
            <Metric label="Sources" value={overview?.source_count ?? 0} />
            <Metric label="Assets" value={overview?.asset_count ?? assets.length} />
            <Metric label="Profiles" value={overview?.profile_count ?? 0} />
            <Metric label="Plan" value={plan ? `${plan.summary.create_count} create` : "Not generated"} />
          </div>

          {plan && (
            <div className="grid grid-cols-5 gap-3">
              <Metric label="Create" value={plan.summary.create_count} />
              <Metric label="Update" value={plan.summary.update_count} />
              <Metric label="Remove" value={plan.summary.remove_count} />
              <Metric label="Skip" value={plan.summary.skip_count} />
              <Metric label="Conflict" value={plan.summary.conflict_count} />
            </div>
          )}

          {executionResult && (
            <div className="grid grid-cols-4 gap-3">
              <Metric label="Executed" value={executionResult.executed_count} />
              <Metric label="Exec Skip" value={executionResult.skipped_count} />
              <Metric label="Exec Conflict" value={executionResult.conflict_count} />
              <Metric label="Errors" value={executionResult.errors.length} />
            </div>
          )}

          {plan && (
            <div className="glass-card overflow-hidden rounded-xl border border-border">
              <div className="flex items-center justify-between border-b border-border px-4 py-3">
                <span className="text-label-caps uppercase text-outline">Deployment Plan</span>
                <span className="font-mono text-body-sm text-primary">{plan.actions.length} actions</span>
              </div>
              <div className="max-h-56 overflow-y-auto">
                {plan.actions.slice(0, 16).map((action) => (
                  <div className="grid grid-cols-[96px_120px_1fr] gap-3 border-b border-border px-4 py-2.5 last:border-b-0" key={action.id}>
                    <span className={planActionClass(action.action_type)}>{action.action_type}</span>
                    <span className="font-mono text-body-sm text-on-surface-variant">{action.profile_id}</span>
                    <div className="min-w-0">
                      <p className="overflow-hidden text-ellipsis whitespace-nowrap font-mono text-body-sm text-on-surface">{action.target_path}</p>
                      <p className="mt-1 overflow-hidden text-ellipsis whitespace-nowrap text-body-sm text-outline">{action.reason}</p>
                    </div>
                  </div>
                ))}
              </div>
            </div>
          )}

          <div className="glass-card overflow-hidden rounded-xl border border-border" aria-label="资产列表">
            {filteredAssets.map((asset) => {
              const isExpanded = expandedIds.has(asset.id);
              return (
                <article
                  className={clsx(
                    "group cursor-pointer border-b border-border transition-colors last:border-b-0 hover:bg-surface-low",
                    isExpanded && "asset-expanded bg-surface-low",
                  )}
                  key={asset.id}
                  onClick={() => toggleAsset(asset.id)}
                >
                  <div className="flex min-h-20 items-start justify-between gap-4 px-4 py-3.5">
                    <div className="min-w-0 flex-1">
                      <div className="flex items-center gap-2">
                        <span className="font-mono text-code-md text-on-surface">{asset.name}</span>
                        <span className={kindBadgeClass(asset.kind)}>{kindLabel[asset.kind] ?? asset.kind}</span>
                        <span className="rounded-md bg-surface-highest px-2 py-0.5 text-[10px] font-bold text-on-surface-variant">
                          本地 Local
                        </span>
                      </div>
                      <button
                        className="asset-description mt-2 block max-w-full font-mono text-body-sm text-on-surface-variant transition-colors hover:text-primary"
                        onClick={(event) => {
                          event.stopPropagation();
                          void revealPath(asset.absolute_path);
                        }}
                        title="在文件管理器中显示"
                      >
                        {displayPath(asset)}
                      </button>
                    </div>
                    <div className="flex gap-2 opacity-0 transition-opacity group-hover:opacity-100" onClick={(event) => event.stopPropagation()}>
                      <button className="grid size-8 place-items-center rounded-lg text-on-surface-variant hover:bg-surface-highest hover:text-primary" aria-label="编辑资产">
                        <Pencil size={17} />
                      </button>
                      <button className="grid size-8 place-items-center rounded-lg text-on-surface-variant hover:bg-surface-highest hover:text-status-remove" aria-label="删除资产">
                        <Trash2 size={17} />
                      </button>
                    </div>
                  </div>

                  {isExpanded && (
                    <div className="grid grid-cols-4 gap-4 border-t border-border/60 bg-surface/60 px-4 pb-4 pt-3">
                      <PathDetail label="Path" value={asset.absolute_path} />
                      <Detail label="Description" value={asset.description ?? "No description"} />
                      <Detail label="Type" value={`${kindLabel[asset.kind]} / ${asset.format}`} />
                      <Detail label="Source" value={asset.source_id} />
                    </div>
                  )}
                </article>
              );
            })}
          </div>
        </section>
      </main>
    </div>
  );
}

function ToolbarButton({ icon, label }: { icon: React.ReactNode; label: string }) {
  return (
    <button className="inline-flex h-9 items-center justify-center gap-2 rounded-xl border border-border bg-surface-high px-3 text-body-sm text-on-surface-variant transition-colors hover:bg-surface-highest hover:text-on-surface">
      {icon}
      <span>{label}</span>
    </button>
  );
}

function IconButton({
  icon,
  label,
  compact = false,
  onClick,
  disabled = false,
}: {
  icon: React.ReactNode;
  label: string;
  compact?: boolean;
  onClick?: () => void;
  disabled?: boolean;
}) {
  return (
    <button
      className={clsx(
        "grid place-items-center text-on-surface-variant transition-colors hover:bg-surface-highest hover:text-on-surface disabled:cursor-not-allowed disabled:opacity-50",
        compact ? "size-7 rounded-lg" : "size-9 rounded-xl border border-border bg-surface-high",
      )}
      aria-label={label}
      onClick={onClick}
      disabled={disabled}
    >
      {icon}
    </button>
  );
}

function Metric({ label, value }: { label: string; value: string | number }) {
  return (
    <div className="flex min-h-14 items-center justify-between rounded-xl border border-border bg-surface-card/60 px-3.5 py-3">
      <span className="text-label-caps uppercase text-outline">{label}</span>
      <strong className="text-h2 font-bold text-primary">{value}</strong>
    </div>
  );
}

function Detail({ label, value, mono = false }: { label: string; value: string; mono?: boolean }) {
  return (
    <div className="min-w-0">
      <span className="block text-label-caps uppercase text-outline">{label}</span>
      <strong className={clsx("mt-1 block overflow-hidden text-ellipsis whitespace-nowrap text-body-sm font-medium text-on-surface", mono && "font-mono text-primary")}>
        {value}
      </strong>
    </div>
  );
}

function PathDetail({ label, value }: { label: string; value: string }) {
  return (
    <div className="min-w-0">
      <span className="block text-label-caps uppercase text-outline">{label}</span>
      <button
        className="mt-1 block max-w-full overflow-hidden text-ellipsis whitespace-nowrap font-mono text-body-sm font-medium text-primary transition-colors hover:text-status-update"
        onClick={(event) => {
          event.stopPropagation();
          void revealPath(value);
        }}
        title="在文件管理器中显示"
      >
        {abbreviateHomePath(value)}
      </button>
    </div>
  );
}

function kindBadgeClass(kind: AssetKind) {
  return clsx(
    "rounded-md px-2 py-0.5 text-[10px] font-bold",
    kind === "skill" && "bg-primary-strong/15 text-primary",
    kind === "rule" && "bg-status-conflict/15 text-status-conflict",
    kind === "agent" && "bg-status-create/15 text-status-create",
    kind !== "skill" && kind !== "rule" && kind !== "agent" && "bg-surface-highest text-on-surface-variant",
  );
}

function displayPath(asset: Asset) {
  return abbreviateHomePath(asset.absolute_path || asset.relative_path);
}

function abbreviateHomePath(path: string) {
  if (path.startsWith("~/") || path === "~" || path.startsWith("%USERPROFILE%/") || path === "%USERPROFILE%") {
    return path;
  }

  const normalizedPath = normalizeSeparators(path);
  const macHomeMatch = normalizedPath.match(/^\/Users\/[^/]+(?=\/|$)/);
  if (macHomeMatch) {
    return normalizedPath.replace(macHomeMatch[0], "~");
  }

  const windowsHomeMatch = normalizedPath.match(/^[A-Za-z]:\/Users\/[^/]+(?=\/|$)/);
  if (windowsHomeMatch) {
    return normalizedPath.replace(windowsHomeMatch[0], "%USERPROFILE%");
  }

  return path;
}

function normalizeSeparators(path: string) {
  return path.split("\\").join("/");
}

function planActionClass(actionType: string) {
  return clsx(
    "rounded-md px-2 py-0.5 text-center text-[10px] font-bold uppercase",
    actionType === "create" && "bg-status-create/15 text-status-create",
    actionType === "update" && "bg-status-update/15 text-status-update",
    actionType === "skip" && "bg-surface-highest text-on-surface-variant",
    actionType === "conflict" && "bg-status-conflict/15 text-status-conflict",
    actionType === "remove" && "bg-status-remove/15 text-status-remove",
  );
}
