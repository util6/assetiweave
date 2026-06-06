import { describe, expect, it } from "vitest";
import { themeCssVars } from "./cssVars";
import type { ThemeDefinition, ThemeTokens } from "./schema";
import { normalizeThemeId, themeOptions, themeRegistry } from "./themes";

const themeIds = ["midnight", "sunlight", "violetDark", "violetLight"] as const;

const tokenGroups = [
  "palette",
  "surface",
  "control",
  "navigation",
  "button",
  "switch",
  "status",
  "effect",
  "component",
] satisfies Array<keyof ThemeTokens>;

const requiredCssVars = [
  "--color-background",
  "--color-surface",
  "--color-surface-lowest",
  "--color-surface-low",
  "--color-surface-card",
  "--color-surface-high",
  "--color-surface-highest",
  "--color-border",
  "--color-outline",
  "--color-outline-variant",
  "--color-on-surface",
  "--color-on-surface-variant",
  "--color-primary",
  "--color-primary-strong",
  "--color-status-update",
  "--color-status-create",
  "--color-status-remove",
  "--color-status-conflict",
  "--color-grid-line",
  "--theme-page-glow",
  "--theme-page-sheen",
  "--theme-grid-opacity",
  "--theme-inset-highlight",
  "--theme-panel-shadow",
  "--theme-glow",
  "--theme-focus-ring",
  "--theme-scrim",
  "--theme-glass-opacity",
  "--theme-hover-lift",
  "--theme-card-bg",
  "--theme-card-border",
  "--theme-card-header",
  "--theme-control-bg",
  "--theme-control-hover",
  "--theme-control-border",
  "--theme-control-fg",
  "--theme-button-primary-bg",
  "--theme-button-primary-hover",
  "--theme-button-primary-fg",
  "--theme-nav-bg",
  "--theme-nav-hover",
  "--theme-nav-active",
  "--theme-nav-active-fg",
  "--theme-nav-active-border",
  "--theme-nav-indicator",
  "--theme-subnav-bg",
  "--theme-toolbar-bg",
  "--theme-switch-bg",
  "--theme-switch-thumb",
  "--theme-switch-checked",
  "--theme-switch-checked-thumb",
  "--theme-shadow-card",
  "--theme-shadow-panel",
  "--theme-shadow-toolbar",
  "--theme-shadow-dialog",
  "--theme-shadow-control-inset",
  "--theme-shadow-active",
] as const;

describe("theme registry", () => {
  it("keeps the theme option order stable", () => {
    expect(themeOptions.map((theme) => theme.id)).toEqual([...themeIds]);
  });

  it("normalizes current and legacy stored theme ids", () => {
    expect(normalizeThemeId("midnight")).toBe("midnight");
    expect(normalizeThemeId("sunlight")).toBe("sunlight");
    expect(normalizeThemeId("violetDark")).toBe("violetDark");
    expect(normalizeThemeId("violetLight")).toBe("violetLight");
    expect(normalizeThemeId("graphite")).toBe("violetDark");
    expect(normalizeThemeId("forest")).toBe("sunlight");
    expect(normalizeThemeId("ember")).toBe("sunlight");
    expect(normalizeThemeId("unknown")).toBe("midnight");
  });

  it("registers complete theme definitions", () => {
    for (const themeId of themeIds) {
      const theme = themeRegistry[themeId];

      expect(theme).toMatchObject<Partial<ThemeDefinition>>({
        id: themeId,
        mode: expect.stringMatching(/^(dark|light)$/),
      });
      expect(theme.swatches).toHaveLength(4);

      for (const group of tokenGroups) {
        expect(theme.tokens[group]).toBeDefined();
        expect(Object.keys(theme.tokens[group]).length).toBeGreaterThan(0);
      }
    }
  });

  it("exports every CSS variable used by Tailwind and foundation recipes", () => {
    for (const themeId of themeIds) {
      const vars = themeCssVars(themeRegistry[themeId]);

      for (const variableName of requiredCssVars) {
        expect(vars[variableName], `${themeId} ${variableName}`).toBeTruthy();
      }
    }
  });
});
