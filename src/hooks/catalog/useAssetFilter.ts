import { useMemo } from "react";
import type { Asset } from "../../types";

export function useAssetFilter(assets: Asset[], query: string) {
  return useMemo(() => {
    const normalized = query.trim().toLowerCase();
    if (!normalized) return assets;

    return assets.filter((asset) =>
      [asset.name, asset.kind, asset.format, asset.relative_path, asset.absolute_path, asset.description ?? ""]
        .join(" ")
        .toLowerCase()
        .includes(normalized),
    );
  }, [assets, query]);
}
