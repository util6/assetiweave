import { useEffect, useState } from "react";
import { fallbackNavigationModel } from "../../fixtures/catalog";
import type { NavigationModel } from "../../navigation/types";
import { getNavigationModel, getOverview, listAppShortcuts, listAssets, listProfiles } from "../../services/catalog";
import type { AppOverview, AppShortcut, Asset, TargetProfile } from "../../types";

export function useCatalogData() {
  const [assets, setAssets] = useState<Asset[]>([]);
  const [overview, setOverview] = useState<AppOverview | null>(null);
  const [profiles, setProfiles] = useState<TargetProfile[]>([]);
  const [appShortcuts, setAppShortcuts] = useState<AppShortcut[]>([]);
  const [navigationModel, setNavigationModel] = useState<NavigationModel>(fallbackNavigationModel);

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

  async function refreshOverview(nextAssets?: Asset[]) {
    const [assetList, appOverview] = await Promise.all([
      nextAssets ? Promise.resolve(nextAssets) : listAssets(),
      getOverview(),
    ]);
    setAssets(assetList);
    setOverview(appOverview);
  }

  return {
    appShortcuts,
    assets,
    navigationModel,
    overview,
    profiles,
    refreshOverview,
  };
}
