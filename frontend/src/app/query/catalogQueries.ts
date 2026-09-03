import { queryOptions } from "@tanstack/react-query";
import {
  getNavigationModel,
  getOverview,
  listAppShortcutSettings,
  listAssetMountStatuses,
  listAssets,
  listProfiles,
  listSources,
} from "../../services/catalog";
import type { AssetKind, Tenant } from "../../types";

export interface QueryScope {
  tenantId: Tenant["id"];
  epoch: number;
}

export const catalogKeys = {
  root: (scope: QueryScope) =>
    ["tenant", scope.tenantId, scope.epoch, "catalog"] as const,
  assetsPrefix: (scope: QueryScope) =>
    [...catalogKeys.root(scope), "assets"] as const,
  assets: (scope: QueryScope, kind?: AssetKind) =>
    [...catalogKeys.root(scope), "assets", kind ?? "all"] as const,

  sources: (scope: QueryScope) =>
    [...catalogKeys.root(scope), "sources"] as const,
  profiles: (scope: QueryScope) =>
    [...catalogKeys.root(scope), "profiles"] as const,
  overview: (scope: QueryScope) =>
    [...catalogKeys.root(scope), "overview"] as const,
  shortcuts: (scope: QueryScope) =>
    [...catalogKeys.root(scope), "shortcuts"] as const,
  mountStatuses: (scope: QueryScope) =>
    [...catalogKeys.root(scope), "mountStatuses"] as const,
  navigation: (scope: QueryScope) =>
    [...catalogKeys.root(scope), "navigation"] as const,
};

const DEFAULT_CATALOG_STALE_TIME = 1000 * 60;

export function assetsQueryOptions(scope: QueryScope, kind?: AssetKind) {
  return queryOptions({
    queryKey: catalogKeys.assets(scope, kind),
    queryFn: () => listAssets(kind),
    networkMode: "always",
    retry: false,
    staleTime: DEFAULT_CATALOG_STALE_TIME,
  });
}

export function sourcesQueryOptions(scope: QueryScope) {
  return queryOptions({
    queryKey: catalogKeys.sources(scope),
    queryFn: () => listSources(),
    networkMode: "always",
    retry: false,
    staleTime: DEFAULT_CATALOG_STALE_TIME,
  });
}

export function profilesQueryOptions(scope: QueryScope) {
  return queryOptions({
    queryKey: catalogKeys.profiles(scope),
    queryFn: () => listProfiles(),
    networkMode: "always",
    retry: false,
    staleTime: DEFAULT_CATALOG_STALE_TIME,
  });
}

export function overviewQueryOptions(scope: QueryScope) {
  return queryOptions({
    queryKey: catalogKeys.overview(scope),
    queryFn: () => getOverview(),
    networkMode: "always",
    retry: false,
    staleTime: DEFAULT_CATALOG_STALE_TIME,
  });
}

export function shortcutsQueryOptions(scope: QueryScope) {
  return queryOptions({
    queryKey: catalogKeys.shortcuts(scope),
    queryFn: () => listAppShortcutSettings(),
    networkMode: "always",
    retry: false,
    staleTime: DEFAULT_CATALOG_STALE_TIME,
  });
}

export function mountStatusesQueryOptions(scope: QueryScope) {
  return queryOptions({
    queryKey: catalogKeys.mountStatuses(scope),
    queryFn: () => listAssetMountStatuses(),
    networkMode: "always",
    retry: false,
    staleTime: DEFAULT_CATALOG_STALE_TIME,
  });
}

export function navigationQueryOptions(scope: QueryScope) {
  return queryOptions({
    queryKey: catalogKeys.navigation(scope),
    queryFn: () => getNavigationModel(),
    networkMode: "always",
    retry: false,
    staleTime: DEFAULT_CATALOG_STALE_TIME,
  });
}
