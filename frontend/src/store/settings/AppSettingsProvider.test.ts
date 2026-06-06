import { describe, expect, it } from "vitest";
import {
  COLUMN_MIN_WIDTH_MAX,
  COLUMN_MIN_WIDTH_MIN,
  DEFAULT_COLUMN_MIN_WIDTH,
  FONT_SIZE_MAX,
  FONT_SIZE_MIN,
  defaultSettings,
  fontFamilyCss,
  normalizeStoredSettings,
} from "./settingsSchema";

describe("AppSettingsProvider", () => {
  it("uses the default column minimum width for older stored settings", () => {
    expect(normalizeStoredSettings({ density: "compact" }).columnMinWidth).toBe(DEFAULT_COLUMN_MIN_WIDTH);
  });

  it("preserves a valid stored column minimum width", () => {
    expect(normalizeStoredSettings({ columnMinWidth: 360 }).columnMinWidth).toBe(360);
  });

  it("clamps stored column minimum width to the supported range", () => {
    expect(normalizeStoredSettings({ columnMinWidth: 120 }).columnMinWidth).toBe(COLUMN_MIN_WIDTH_MIN);
    expect(normalizeStoredSettings({ columnMinWidth: 900 }).columnMinWidth).toBe(COLUMN_MIN_WIDTH_MAX);
  });

  it("adds typography and conversation defaults when migrating older settings", () => {
    const settings = normalizeStoredSettings({ density: "compact", theme: "sunlight" });

    expect(settings.typography).toEqual(defaultSettings.typography);
    expect(settings.conversations).toEqual(defaultSettings.conversations);
  });

  it("preserves custom page-level typography overrides", () => {
    const contentFontFamily = '"Inter Variable", ui-sans-serif, system-ui, sans-serif';
    const codeFontFamily = '"Maple Mono NF CN", "JetBrains Mono", monospace';
    const sessionBrowserFontFamily = '"LXGW WenKai", Georgia, serif';
    const settings = normalizeStoredSettings({
      typography: {
        codeFontFamily,
        contentFontFamily,
        contentFontSize: 16,
        interfaceFontFamily: "SF Pro Display, system-ui, sans-serif",
      },
      conversations: {
        contentFontFamily: "Atkinson Hyperlegible, sans-serif",
        contentFontSize: 15,
        sessionBrowserFontFamily,
        sessionBrowserFontSize: 12,
        sessionToolbarCompact: false,
      },
    });

    expect(settings.typography.codeFontFamily).toBe(codeFontFamily);
    expect(settings.typography.contentFontFamily).toBe(contentFontFamily);
    expect(settings.typography.contentFontSize).toBe(16);
    expect(settings.typography.interfaceFontFamily).toBe("SF Pro Display, system-ui, sans-serif");
    expect(settings.conversations.contentFontFamily).toBe("Atkinson Hyperlegible, sans-serif");
    expect(settings.conversations.contentFontSize).toBe(15);
    expect(settings.conversations.sessionBrowserFontFamily).toBe(sessionBrowserFontFamily);
    expect(settings.conversations.sessionBrowserFontSize).toBe(12);
    expect(settings.conversations.sessionToolbarCompact).toBe(false);
  });

  it("migrates legacy font tokens to editable CSS font-family values", () => {
    const settings = normalizeStoredSettings({
      typography: {
        codeFontFamily: "mono",
        contentFontFamily: "serif",
        interfaceFontFamily: "geist",
      },
      conversations: {
        contentFontFamily: "system",
        sessionBrowserFontFamily: "mono",
      },
    });

    expect(settings.typography.codeFontFamily).toBe(fontFamilyCss.mono);
    expect(settings.typography.contentFontFamily).toBe(fontFamilyCss.serif);
    expect(settings.typography.interfaceFontFamily).toBe(fontFamilyCss.geist);
    expect(settings.conversations.contentFontFamily).toBe(fontFamilyCss.system);
    expect(settings.conversations.sessionBrowserFontFamily).toBe(fontFamilyCss.mono);
  });

  it("normalizes invalid typography values", () => {
    const settings = normalizeStoredSettings({
      typography: {
        baseFontSize: 99,
        codeFontFamily: "Arial; color: red",
      },
      conversations: {
        contentFontSize: 2,
        sessionBrowserFontFamily: "Bad { font-family: serif }",
      },
    });

    expect(settings.typography.baseFontSize).toBe(FONT_SIZE_MAX);
    expect(settings.typography.codeFontFamily).toBe(defaultSettings.typography.codeFontFamily);
    expect(settings.conversations.contentFontSize).toBe(FONT_SIZE_MIN);
    expect(settings.conversations.sessionBrowserFontFamily).toBe(defaultSettings.typography.contentFontFamily);
  });
});
