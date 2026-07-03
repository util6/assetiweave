import { useMemo } from "react";
import type { Asset, AssetKind } from "../../types";

export type AssetSortBy = "created" | "updated" | "name" | "kind";
export type AssetSortDirection = "asc" | "desc";

export interface AssetFilterOptions {
  kindFilters: AssetKind[];
  query: string;
  sortBy: AssetSortBy;
  sortDirection: AssetSortDirection;
  sourceFilters: string[];
}

export function useAssetFilter(assets: Asset[], options: AssetFilterOptions) {
  return useMemo(() => {
    return filterAssets(assets, options);
  }, [assets, options]);
}

export function filterAssets(assets: Asset[], options: AssetFilterOptions) {
  const normalized = options.query.trim().toLowerCase();
  const kindFilters = new Set(options.kindFilters);
  const sourceFilters = new Set(options.sourceFilters);

  return assets
    .filter((asset) => {
      if (kindFilters.size > 0 && !kindFilters.has(asset.kind)) {
        return false;
      }

      if (sourceFilters.size > 0 && !sourceFilters.has(asset.source_id)) {
        return false;
      }

      if (!normalized) {
        return true;
      }

      return [asset.name, asset.kind, asset.format, asset.relative_path, asset.absolute_path, asset.description ?? ""]
        .join(" ")
        .toLowerCase()
        .includes(normalized);
    })
    .sort((first, second) => compareAssets(first, second, options.sortBy, options.sortDirection));
}

function compareAssets(first: Asset, second: Asset, sortBy: AssetSortBy, direction: AssetSortDirection) {
  const directionMultiplier = direction === "asc" ? 1 : -1;
  const primary = compareAssetField(first, second, sortBy);

  if (primary !== 0) {
    return primary * directionMultiplier;
  }

  return first.name.localeCompare(second.name) || first.id.localeCompare(second.id);
}

function compareAssetField(first: Asset, second: Asset, sortBy: AssetSortBy) {
  if (sortBy === "created") {
    return compareDate(first.discovered_at, second.discovered_at);
  }

  if (sortBy === "updated") {
    return compareDate(first.updated_at, second.updated_at);
  }

  if (sortBy === "kind") {
    return first.kind.localeCompare(second.kind);
  }

  return first.name.localeCompare(second.name);
}

function compareDate(first: string, second: string) {
  const firstTime = Date.parse(first);
  const secondTime = Date.parse(second);

  if (!Number.isFinite(firstTime) && !Number.isFinite(secondTime)) {
    return 0;
  }
  if (!Number.isFinite(firstTime)) {
    return -1;
  }
  if (!Number.isFinite(secondTime)) {
    return 1;
  }

  return firstTime - secondTime;
}
