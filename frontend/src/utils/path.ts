import type { Asset } from "../types";

export function displayAssetPath(asset: Asset) {
  return asset.display_path || abbreviateHomePath(asset.absolute_path || asset.relative_path);
}

export function abbreviateHomePath(path: string) {
  if (path.startsWith("~/") || path === "~") {
    return path;
  }

  const normalizedPath = normalizeSeparators(path);
  if (normalizedPath.startsWith("%USERPROFILE%/") || normalizedPath === "%USERPROFILE%") {
    return normalizedPath.replace("%USERPROFILE%", "~");
  }

  const macHomeMatch = normalizedPath.match(/^\/Users\/[^/]+(?=\/|$)/);
  if (macHomeMatch) {
    return normalizedPath.replace(macHomeMatch[0], "~");
  }

  const linuxHomeMatch = normalizedPath.match(/^\/home\/[^/]+(?=\/|$)/);
  if (linuxHomeMatch) {
    return normalizedPath.replace(linuxHomeMatch[0], "~");
  }

  const windowsHomeMatch = normalizedPath.match(/^[A-Za-z]:\/Users\/[^/]+(?=\/|$)/);
  if (windowsHomeMatch) {
    return normalizedPath.replace(windowsHomeMatch[0], "~");
  }

  return path;
}

function normalizeSeparators(path: string) {
  return path.split("\\").join("/");
}
