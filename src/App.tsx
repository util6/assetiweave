import { useEffect, useMemo, useState } from "react";
import clsx from "clsx";
import {
  Archive,
  Check,
  Download,
  Eye,
  Filter,
  Folder,
  Grid3X3,
  Languages,
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
import { NotificationBanner, type NotificationMessage } from "./components/notifications/NotificationBanner";
import { assetKindLabel, deploymentActionLabel, translatePlanReason, translateScanStatus } from "./i18n/domain";
import { useI18n } from "./i18n/I18nProvider";
import { navigationModel as fallbackNavigationModel } from "./navigation/menu";
import type { NavigationModel } from "./navigation/types";
import {
  createPlan,
  executePlan,
  getNavigationModel,
  getOverview,
  listAssets,
  listAppShortcuts,
  listProfiles,
  revealPath,
  scanSources,
} from "./services/catalog";
import type { AppOverview, AppShortcut, Asset, AssetKind, DeploymentPlan, ExecutionResult, TargetProfile } from "./types";

export function App() {
  const { t } = useI18n();
  const [assets, setAssets] = useState<Asset[]>([]);
  const [overview, setOverview] = useState<AppOverview | null>(null);
  const [profiles, setProfiles] = useState<TargetProfile[]>([]);
  const [appShortcuts, setAppShortcuts] = useState<AppShortcut[]>([]);
  const [plan, setPlan] = useState<DeploymentPlan | null>(null);
  const [executionResult, setExecutionResult] = useState<ExecutionResult | null>(null);
  const [navigationModel, setNavigationModel] = useState<NavigationModel>(fallbackNavigationModel);
  const [notification, setNotification] = useState<NotificationMessage | null>({
    id: "mvp-notification-outlet",
    tone: "success",
    messageKey: "notification.ready",
  });
  const [busy, setBusy] = useState(false);
  const [expandedIds, setExpandedIds] = useState<Set<string>>(new Set());
  const [selectedMounts, setSelectedMounts] = useState<Record<string, string[]>>({});
  const [query, setQuery] = useState("");
  const activeSubNavItems = navigationModel.subNavItems[navigationModel.activeHeaderTabId] ?? [];

  useEffect(() => {
    void Promise.all([listAssets(), getOverview(), getNavigationModel(), listProfiles(), listAppShortcuts()]).then(
      ([assetList, appOverview, appNavigationModel, profileList, shortcutList]) => {
        setAssets(assetList);
        setOverview(appOverview);
        setNavigationModel(appNavigationModel);
        setProfiles(profileList);
        setAppShortcuts(shortcutList);
      },
    );
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

  function toggleMountProfile(assetId: string, profileId: string) {
    setSelectedMounts((current) => {
      const selected = new Set(current[assetId] ?? []);
      if (selected.has(profileId)) {
        selected.delete(profileId);
      } else {
        selected.add(profileId);
      }
      return {
        ...current,
        [assetId]: [...selected],
      };
    });
  }

  function dismissNotification(id: string) {
    setNotification((current) => (current?.id === id ? null : current));
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
        <header className="sticky top-0 z-20 grid h-16 shrink-0 grid-cols-[minmax(180px,1fr)_auto_minmax(360px,1fr)] items-center gap-4 px-8 backdrop-blur">
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

        <SubNavigation activeId={navigationModel.activeSubNavId} items={activeSubNavItems} />

        <NotificationBanner notification={notification} onDismiss={dismissNotification} />

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
                onChange={(event) => setQuery(event.target.value)}
              />
            </label>
            <div className="flex h-9 items-center gap-1 rounded-xl border border-border bg-surface-high p-1">
              <IconButton label={t("toolbar.view.compact")} icon={<Menu size={17} />} compact />
              <button className="grid size-7 place-items-center rounded-lg bg-status-update text-white" aria-label={t("toolbar.view.list")}>
                <List size={17} />
              </button>
              <IconButton label={t("toolbar.view.grid")} icon={<Grid3X3 size={17} />} compact />
            </div>
            <ToolbarButton icon={<Filter size={17} />} label={t("toolbar.filter.all", { count: overview?.asset_count ?? assets.length })} />
            <ToolbarButton icon={<Tag size={17} />} label={t("toolbar.filter.tag")} />
            <ToolbarButton icon={<SlidersHorizontal size={17} />} label={t("toolbar.sort.createdAt")} />
            <IconButton label={t("toolbar.export")} icon={<Download size={17} />} />
          </div>

          <div className="flex items-center gap-2 max-[1160px]:flex-wrap">
            <button
              className="grid size-10 place-items-center rounded-xl bg-gradient-to-br from-status-update to-status-create/70 text-white shadow-glow transition-transform hover:-translate-y-0.5 active:scale-95"
              aria-label={t("toolbar.createDeploymentPlan")}
              onClick={handleCreatePlan}
              disabled={busy}
            >
              <Plus size={22} />
            </button>
            <span className="mx-1 h-6 w-px bg-border" />
            <IconButton label={t("toolbar.scanSources")} icon={<RefreshCw size={17} />} onClick={handleScan} disabled={busy} />
            <IconButton label={t("toolbar.generatePlan")} icon={<Eye size={17} />} onClick={handleCreatePlan} disabled={busy} />
            <IconButton label={t("toolbar.executePlan")} icon={<Upload size={17} />} onClick={handleExecutePlan} disabled={busy || !plan} />
            <IconButton label={t("toolbar.openFolder")} icon={<Folder size={17} />} />
            <IconButton label={t("toolbar.settings")} icon={<Settings size={17} />} />
          </div>
        </section>

        <section className="flex flex-1 flex-col gap-4 px-8 py-6">
          <div className="grid grid-cols-4 gap-3">
            <Metric label={t("metric.sources")} value={overview?.source_count ?? 0} />
            <Metric label={t("metric.assets")} value={overview?.asset_count ?? assets.length} />
            <Metric label={t("metric.profiles")} value={overview?.profile_count ?? 0} />
            <Metric label={t("metric.plan")} value={plan ? t("plan.createSummary", { count: plan.summary.create_count }) : t("plan.notGenerated")} />
          </div>

          {plan && (
            <div className="grid grid-cols-5 gap-3">
              <Metric label={t("metric.create")} value={plan.summary.create_count} />
              <Metric label={t("metric.update")} value={plan.summary.update_count} />
              <Metric label={t("metric.remove")} value={plan.summary.remove_count} />
              <Metric label={t("metric.skip")} value={plan.summary.skip_count} />
              <Metric label={t("metric.conflict")} value={plan.summary.conflict_count} />
            </div>
          )}

          {executionResult && (
            <div className="grid grid-cols-4 gap-3">
              <Metric label={t("metric.executed")} value={executionResult.executed_count} />
              <Metric label={t("metric.execSkip")} value={executionResult.skipped_count} />
              <Metric label={t("metric.execConflict")} value={executionResult.conflict_count} />
              <Metric label={t("metric.errors")} value={executionResult.errors.length} />
            </div>
          )}

          {plan && (
            <div className="glass-card overflow-hidden rounded-xl border border-border">
              <div className="flex items-center justify-between border-b border-border px-4 py-3">
                <span className="text-label-caps uppercase text-outline">{t("plan.title")}</span>
                <span className="font-mono text-body-sm text-primary">{t("plan.actions", { count: plan.actions.length })}</span>
              </div>
              <div className="max-h-56 overflow-y-auto">
                {plan.actions.slice(0, 16).map((action) => (
                  <div className="grid grid-cols-[96px_120px_1fr] gap-3 border-b border-border px-4 py-2.5 last:border-b-0" key={action.id}>
                    <span className={planActionClass(action.action_type)}>{deploymentActionLabel(action.action_type, t)}</span>
                    <span className="font-mono text-body-sm text-on-surface-variant">{action.profile_id}</span>
                    <div className="min-w-0">
                      <p className="overflow-hidden text-ellipsis whitespace-nowrap font-mono text-body-sm text-on-surface">{action.target_path}</p>
                      <p className="mt-1 overflow-hidden text-ellipsis whitespace-nowrap text-body-sm text-outline">{translatePlanReason(action.reason, t)}</p>
                    </div>
                  </div>
                ))}
              </div>
            </div>
          )}

          <div className="glass-card overflow-hidden rounded-xl border border-border" aria-label={t("asset.list.aria")}>
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
                  <div className="relative flex min-h-28 items-start justify-between gap-4 px-4 py-3.5">
                    <div className="min-w-0 flex-1 pr-80">
                      <div className="flex items-center gap-2">
                        <span className="font-mono text-code-md text-on-surface">{asset.name}</span>
                        <span className={kindBadgeClass(asset.kind)}>{assetKindLabel(asset.kind, t)}</span>
                        <span className="rounded-md bg-surface-highest px-2 py-0.5 text-[10px] font-bold text-on-surface-variant">
                          {t("asset.origin.local")}
                        </span>
                      </div>
                      <button
                        className="asset-description mt-2 block max-w-full font-mono text-body-sm text-on-surface-variant transition-colors hover:text-primary"
                        onClick={(event) => {
                          event.stopPropagation();
                          void revealPath(asset.absolute_path);
                        }}
                        title={t("asset.revealPath")}
                      >
                        {displayPath(asset)}
                      </button>
                      <div className="mt-3 flex min-w-0 items-start gap-4 max-[980px]:flex-col max-[980px]:gap-2">
                        <InlineMeta label={t("asset.description")} value={asset.description ?? t("asset.noDescription")} />
                        <InlineMeta label={t("asset.source")} value={asset.source_id} mono />
                      </div>
                    </div>
                    <div className="absolute right-4 top-3.5 flex w-72 justify-end gap-3" onClick={(event) => event.stopPropagation()}>
                      <QuickMountButtons
                        asset={asset}
                        profiles={profiles}
                        shortcuts={appShortcuts}
                        selectedProfileIds={selectedMounts[asset.id] ?? []}
                        onToggle={(profileId) => toggleMountProfile(asset.id, profileId)}
                      />
                      <button className="grid size-8 place-items-center rounded-lg text-on-surface-variant hover:bg-surface-highest hover:text-primary" aria-label={t("asset.edit")}>
                        <Pencil size={17} />
                      </button>
                      <button className="grid size-8 place-items-center rounded-lg text-on-surface-variant hover:bg-surface-highest hover:text-status-remove" aria-label={t("asset.delete")}>
                        <Trash2 size={17} />
                      </button>
                    </div>
                  </div>

                  {isExpanded && (
                    <MountSelector
                      asset={asset}
                      profiles={profiles}
                      selectedProfileIds={selectedMounts[asset.id] ?? []}
                      onToggle={(profileId) => toggleMountProfile(asset.id, profileId)}
                    />
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

function InlineMeta({ label, value, mono = false }: { label: string; value: string; mono?: boolean }) {
  return (
    <div className={clsx("flex min-w-0 items-baseline gap-2", mono ? "max-w-56 shrink-0" : "max-w-[520px] flex-1")}>
      <span className="shrink-0 text-label-caps uppercase text-outline">{label}</span>
      <span
        className={clsx(
          "block min-w-0 overflow-hidden text-ellipsis whitespace-nowrap text-body-sm font-semibold text-on-surface",
          mono && "font-mono text-primary",
        )}
        title={value}
      >
        {value}
      </span>
    </div>
  );
}

function LanguageSwitcher() {
  const { locale, setLocale, t } = useI18n();

  return (
    <div
      className="flex h-9 shrink-0 items-center gap-1 rounded-xl border border-border bg-surface-high p-1 text-body-sm"
      aria-label={t("language.label")}
      role="group"
    >
      <Languages size={16} className="mx-1 text-outline" aria-hidden="true" />
      {(["zh", "en"] as const).map((nextLocale) => (
        <button
          className={clsx(
            "h-7 rounded-lg px-2.5 font-semibold transition-colors",
            locale === nextLocale ? "bg-surface-highest text-primary" : "text-on-surface-variant hover:text-on-surface",
          )}
          key={nextLocale}
          onClick={() => setLocale(nextLocale)}
          type="button"
        >
          {t(nextLocale === "zh" ? "language.zh" : "language.en")}
        </button>
      ))}
    </div>
  );
}

function QuickMountButtons({
  asset,
  profiles,
  shortcuts,
  selectedProfileIds,
  onToggle,
}: {
  asset: Asset;
  profiles: TargetProfile[];
  shortcuts: AppShortcut[];
  selectedProfileIds: string[];
  onToggle: (profileId: string) => void;
}) {
  const { t } = useI18n();

  return (
    <div className="flex min-w-0 items-center justify-end gap-1.5">
      {shortcuts
        .filter((shortcut) => shortcut.enabled)
        .map((shortcut) => {
          const profile = profiles.find((candidate) => candidate.id === shortcut.profileId);
          const selected = selectedProfileIds.includes(shortcut.profileId);
          const supported = profile?.supported_kinds.includes(asset.kind) ?? true;
          return (
            <button
              className={clsx(
                "grid size-8 place-items-center rounded-full border text-[13px] font-bold transition-all",
                selected ? "shadow-glow" : "opacity-55 hover:opacity-100",
                !supported && "grayscale",
              )}
              key={shortcut.profileId}
              onClick={() => onToggle(shortcut.profileId)}
              style={{
                borderColor: selected ? shortcut.accentColor : `${shortcut.accentColor}55`,
                backgroundColor: selected ? `${shortcut.accentColor}24` : "transparent",
                color: shortcut.accentColor,
              }}
              title={t(selected ? "mount.unmount" : "mount.mountTo", { profile: shortcut.profileName })}
              type="button"
            >
              {shortcut.displayIcon}
            </button>
          );
        })}
    </div>
  );
}

function MountSelector({
  asset,
  profiles,
  selectedProfileIds,
  onToggle,
}: {
  asset: Asset;
  profiles: TargetProfile[];
  selectedProfileIds: string[];
  onToggle: (profileId: string) => void;
}) {
  const { t } = useI18n();
  const enabledProfiles = profiles.filter((profile) => profile.enabled);

  return (
    <div className="border-t border-border/60 bg-surface/60 px-4 pb-4 pt-3" onClick={(event) => event.stopPropagation()}>
      <div className="mb-3 flex items-center justify-between gap-4">
        <div className="min-w-0">
          <span className="text-label-caps uppercase text-outline">{t("mount.title")}</span>
          <p className="mt-1 overflow-hidden text-ellipsis whitespace-nowrap text-body-sm text-on-surface-variant">
            {t("mount.description")}
          </p>
        </div>
        <span className="rounded-md border border-border bg-surface-high px-2.5 py-1 font-mono text-body-sm text-primary">
          {t("mount.selected", { count: selectedProfileIds.length })}
        </span>
      </div>

      {enabledProfiles.length === 0 ? (
        <div className="rounded-lg border border-border bg-surface-high px-3 py-3 text-body-sm text-on-surface-variant">
          {t("mount.empty")}
        </div>
      ) : (
        <div className="grid grid-cols-4 gap-2.5 max-[980px]:grid-cols-2 max-[720px]:grid-cols-1">
          {enabledProfiles.map((profile) => {
            const selected = selectedProfileIds.includes(profile.id);
            const supported = profile.supported_kinds.includes(asset.kind);
            return (
              <button
                className={clsx(
                  "min-h-16 rounded-lg border bg-surface-high px-3 py-2.5 text-left transition-all",
                  selected
                    ? "border-status-create/70 bg-status-create/12 shadow-glow"
                    : "border-border hover:border-outline-variant hover:bg-surface-highest",
                  !supported && "opacity-60",
                )}
                key={profile.id}
                onClick={() => onToggle(profile.id)}
                type="button"
              >
                <div className="flex items-center justify-between gap-2">
                  <div className="min-w-0">
                    <p className="overflow-hidden text-ellipsis whitespace-nowrap text-body-sm font-bold text-on-surface">{profile.name}</p>
                    <p className="mt-1 overflow-hidden text-ellipsis whitespace-nowrap font-mono text-code-sm text-on-surface-variant">
                      {abbreviateHomePath(profile.target_paths[0] ?? "")}
                    </p>
                  </div>
                  <span
                    className={clsx(
                      "grid size-6 shrink-0 place-items-center rounded-full border transition-colors",
                      selected ? "border-status-create bg-status-create text-background" : "border-border text-transparent",
                    )}
                  >
                    <Check size={15} />
                  </span>
                </div>
                <p className={clsx("mt-2 overflow-hidden text-ellipsis whitespace-nowrap text-body-sm", supported ? "text-status-create" : "text-status-conflict")}>
                  {t(supported ? "mount.supported" : "mount.unsupported")}
                </p>
              </button>
            );
          })}
        </div>
      )}
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
