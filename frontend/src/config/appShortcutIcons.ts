import appShortcutIconAssets from "../../../assets/app-icons/icons";
import type { AppShortcutIconPath, AppShortcutIconSvg } from "../types";

export interface AppShortcutIconAsset {
  legacyIcon: string;
  accentColor: string;
  svg: string;
}

export interface AppShortcutIconDefinition extends AppShortcutIconSvg {
  legacyIcon?: string;
}

export interface AppShortcutIconCatalogItem {
  appKind: string;
  asset: AppShortcutIconAsset;
  definition: AppShortcutIconDefinition;
}

export const appShortcutIconAssetsByKind = appShortcutIconAssets as Record<string, AppShortcutIconAsset>;

export function scanAppShortcutIcons(assets: Record<string, AppShortcutIconAsset>): AppShortcutIconCatalogItem[] {
  return Object.entries(assets).map(([appKind, asset]) => ({
    appKind,
    asset,
    definition: { ...parseSvgIcon(asset.svg), legacyIcon: asset.legacyIcon },
  }));
}

// The central object is the only built-in icon registry. Adding an entry there
// automatically feeds both icon rendering and the global settings picker.
export const appShortcutIconCatalog = scanAppShortcutIcons(appShortcutIconAssetsByKind);

export const appShortcutIconAccentColors = Object.fromEntries(
  appShortcutIconCatalog.map(({ appKind, asset }) => [appKind, asset.accentColor]),
) as Record<string, string>;

export const appShortcutIcons = Object.fromEntries(
  appShortcutIconCatalog.map(({ appKind, definition }) => [appKind, definition]),
) as Record<string, AppShortcutIconDefinition>;

function parseSvgIcon(source: string): AppShortcutIconSvg {
  const rootMatch = source.match(/<svg\b([^>]*)>/i);
  if (!rootMatch) {
    throw new Error("App icon asset is not valid SVG");
  }

  const paths = Array.from(source.matchAll(/<path\b([^>]*)\/?\s*>/gi)).map(([, attributes]) => {
    const d = readSvgAttribute(attributes, "d")?.trim();
    if (!d) {
      throw new Error("App icon SVG path is missing its d attribute");
    }

    const result: AppShortcutIconPath = { d };
    const clipRule = readSvgAttribute(attributes, "clip-rule");
    const fillRule = readSvgAttribute(attributes, "fill-rule");
    if (clipRule === "evenodd" || clipRule === "nonzero") {
      result.clipRule = clipRule;
    }
    if (fillRule === "evenodd" || fillRule === "nonzero") {
      result.fillRule = fillRule;
    }
    return result;
  });

  if (paths.length === 0) {
    throw new Error("App icon SVG must contain at least one path");
  }

  return {
    paths,
    viewBox: readSvgAttribute(rootMatch[1], "viewBox")?.trim() || undefined,
  };
}

function readSvgAttribute(attributes: string, name: string) {
  const match = attributes.match(new RegExp(`${name}\\s*=\\s*(["'])(.*?)\\1`, "i"));
  return match?.[2];
}
