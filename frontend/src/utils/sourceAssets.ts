import type { Asset, Source } from "../types";

const SKILL_BACKUP_SOURCE_ID = "assetiweave-library-skills";

export function groupSourceDisplayAssets(assets: Asset[], sources: Source[]) {
  const grouped = assets.reduce<Map<string, Asset[]>>((current, asset) => {
    if (asset.kind !== "skill") {
      return current;
    }

    current.set(asset.source_id, [...(current.get(asset.source_id) ?? []), asset]);
    return current;
  }, new Map<string, Asset[]>());

  const backupSource = sources.find(isSkillBackupSource);
  if (!backupSource) {
    return grouped;
  }

  const existingAssetIds = new Set(assets.map((asset) => asset.id));
  const displayedBackupIds = new Set((grouped.get(backupSource.id) ?? []).map((asset) => asset.id));
  for (const asset of assets) {
    const backupPath = asset.backup_status?.backup_path;
    const hiddenBackupId = asset.backup_status?.hidden_asset_ids.find((assetId) => !existingAssetIds.has(assetId));
    if (!backupPath || !hiddenBackupId || asset.source_id === backupSource.id || displayedBackupIds.has(hiddenBackupId)) {
      continue;
    }

    const backupAsset: Asset = {
      ...asset,
      id: hiddenBackupId,
      source_id: backupSource.id,
      absolute_path: backupPath,
      relative_path: backupRelativePath(backupPath, backupSource.root_path),
      repository: null,
      backup_status: {
        state: asset.backup_status?.state ?? "backed_up",
        backup_path: backupPath,
        hidden_asset_ids: [],
      },
    };
    grouped.set(backupSource.id, [...(grouped.get(backupSource.id) ?? []), backupAsset]);
    displayedBackupIds.add(hiddenBackupId);
  }

  return grouped;
}

function isSkillBackupSource(source: Source) {
  return source.id === SKILL_BACKUP_SOURCE_ID || source.source_origin === "assetiweave_library";
}

function backupRelativePath(backupPath: string, rootPath: string) {
  const normalizedBackupPath = backupPath.replace(/\\/g, "/");
  const normalizedRootPath = rootPath.replace(/\\/g, "/").replace(/\/$/, "");
  if (normalizedRootPath && normalizedBackupPath.startsWith(`${normalizedRootPath}/`)) {
    return normalizedBackupPath.slice(normalizedRootPath.length + 1);
  }
  return normalizedBackupPath;
}
