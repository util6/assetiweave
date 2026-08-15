/* @vitest-environment jsdom */

import { describe, expect, it } from "vitest";
import {
  appShortcutIconAssetsByKind,
  appShortcutIconCatalog,
  appShortcutIcons,
  scanAppShortcutIcons,
} from "./appShortcutIcons";

describe("app shortcut icon assets", () => {
  it("exposes every built-in app icon declared in the central source file", () => {
    const assetKeys = Object.keys(appShortcutIconAssetsByKind).sort();
    const iconKeys = Object.keys(appShortcutIcons).sort();

    expect(iconKeys).toEqual(assetKeys);
    expect(iconKeys).toEqual(
      expect.arrayContaining([
        "antigravity",
        "claude",
        "codex",
        "cursor",
        "gemini",
        "hermes",
        "kiro",
        "openclaw",
        "opencode",
        "qoder",
        "zcode",
      ]),
    );
    expect(iconKeys.length).toBeGreaterThanOrEqual(11);
  });

  it("preserves path data and legacy display icon compatibility", () => {
    for (const [appKind, definition] of Object.entries(appShortcutIcons)) {
      expect(definition.paths.length, appKind).toBeGreaterThan(0);
      expect(definition.viewBox, appKind).toMatch(/^0 0 \d+ \d+$/);
      expect(appShortcutIconAssetsByKind[appKind].legacyIcon, appKind).toBeTruthy();
      expect(appShortcutIconAssetsByKind[appKind].svg, appKind).toContain("<svg");
    }
  });

  it("scans a newly added source entry without a separate registry update", () => {
    const catalog = scanAppShortcutIcons({
      "sample-agent": {
        legacyIcon: "S",
        svg: '<svg viewBox="0 0 24 24"><path d="M0 0" /></svg>',
      },
    });

    expect(catalog.map((item) => item.appKind)).toEqual(["sample-agent"]);
    expect(catalog[0]?.asset.legacyIcon).toBe("S");
    expect(catalog[0]?.definition.paths[0]?.d).toBe("M0 0");
    expect(appShortcutIconCatalog.map((item) => item.appKind)).toEqual(Object.keys(appShortcutIconAssetsByKind));
  });
});
