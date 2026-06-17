import type { Asset } from "../types";

export function getBackupableSkillAssets(assets: Asset[]): Asset[] {
  return assets.filter(isBackupableSkillAsset);
}

export function getBackupableSkillAssetsByIds(assetsById: Map<string, Asset>, assetIds: Iterable<string>): Asset[] {
  const backupableAssets: Asset[] = [];
  const seenAssetIds = new Set<string>();

  for (const assetId of assetIds) {
    if (seenAssetIds.has(assetId)) {
      continue;
    }
    seenAssetIds.add(assetId);

    const asset = assetsById.get(assetId);
    if (asset && isBackupableSkillAsset(asset)) {
      backupableAssets.push(asset);
    }
  }

  return backupableAssets;
}

function isBackupableSkillAsset(asset: Asset) {
  return asset.kind === "skill" && !asset.backup_status;
}
