import type { Asset } from "../types";

export function displayAssetPath(asset: Asset) {
  return abbreviateHomePath(asset.absolute_path || asset.relative_path);
}

export function abbreviateHomePath(path: string) {
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
