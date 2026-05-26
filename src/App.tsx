import { useEffect, useMemo, useState } from "react";
import { AssetList } from "./components/assets/AssetList";
import { AppHeader } from "./components/layout/AppHeader";
import { AssetToolbar } from "./components/layout/AssetToolbar";
import { SideRail } from "./components/navigation/SideRail";
import { SubNavigation } from "./components/navigation/SubNavigation";
import { NotificationBanner, type NotificationMessage } from "./components/notifications/NotificationBanner";
import { DashboardMetrics } from "./components/plans/DashboardMetrics";
import { DeploymentPlanPanel } from "./components/plans/DeploymentPlanPanel";
import { navigationModel as fallbackNavigationModel } from "./navigation/menu";
import type { NavigationModel } from "./navigation/types";
import {
  createPlan,
  executePlan,
  getNavigationModel,
  getOverview,
  listAppShortcuts,
  listAssets,
  listProfiles,
  revealPath,
  scanSources,
} from "./services/catalog";
import type { AppOverview, AppShortcut, Asset, DeploymentPlan, ExecutionResult, TargetProfile } from "./types";

export function App() {
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
        <AppHeader navigationModel={navigationModel} overview={overview} />
        <SubNavigation activeId={navigationModel.activeSubNavId} items={activeSubNavItems} />
        <NotificationBanner notification={notification} onDismiss={dismissNotification} />
        <AssetToolbar
          assetCount={overview?.asset_count ?? assets.length}
          busy={busy}
          hasPlan={Boolean(plan)}
          onCreatePlan={handleCreatePlan}
          onExecutePlan={handleExecutePlan}
          onQueryChange={setQuery}
          onScan={handleScan}
          query={query}
        />

        <section className="flex flex-1 flex-col gap-4 px-8 py-6">
          <DashboardMetrics assets={assets} executionResult={executionResult} overview={overview} plan={plan} />
          <DeploymentPlanPanel plan={plan} />
          <AssetList
            appShortcuts={appShortcuts}
            assets={filteredAssets}
            expandedIds={expandedIds}
            onRevealPath={(path) => void revealPath(path)}
            onToggleAsset={toggleAsset}
            onToggleMount={toggleMountProfile}
            profiles={profiles}
            selectedMounts={selectedMounts}
          />
        </section>
      </main>
    </div>
  );
}
