import type { Asset, Source } from "../types";

export function assetSourceHref(asset: Asset) {
  return asset.repository?.web_url ?? undefined;
}

export function assetSourceLabel(asset: Asset, source?: Source) {
  const sourceName = source?.name.trim();

  return asset.repository?.remote_url
    ?? asset.repository?.root_path
    ?? (sourceName && sourceName !== source?.id ? sourceName : undefined)
    ?? source?.root_path
    ?? asset.source_id;
}
