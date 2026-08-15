/* @vitest-environment jsdom */

import { describe, expect, it } from "vitest";
import { appShortcutIconAssetsByKind, appShortcutIcons } from "./appShortcutIcons";

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
});
