import { describe, expect, it } from "vitest";
import {
  COLUMN_MIN_WIDTH_MAX,
  COLUMN_MIN_WIDTH_MIN,
  DEFAULT_COLUMN_MIN_WIDTH,
  FONT_SIZE_MAX,
  FONT_SIZE_MIN,
  RESULT_PREVIEW_LINE_LIMIT_MAX,
  RESULT_PREVIEW_LINE_LIMIT_MIN,
  createFontFamilySetting,
  defaultSettings,
  fontFamilyCss,
  normalizeStoredSettings,
  resolveFontFamilyCss,
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
    expect(settings.dataBackup).toEqual(defaultSettings.dataBackup);
    expect(settings.conversationRuntimeOverrides).toEqual(defaultSettings.conversationRuntimeOverrides);
  });

  it("preserves configured conversation runtime override paths", () => {
    const settings = normalizeStoredSettings({
      conversationRuntimeOverrides: {
        bash: "  /opt/homebrew/bin/bash  ",
        node: "/opt/homebrew/bin/node",
        python: "C:\\Python312\\python.exe",
      },
    });

    expect(settings.conversationRuntimeOverrides).toEqual({
      bash: "/opt/homebrew/bin/bash",
      node: "/opt/homebrew/bin/node",
      python: "C:\\Python312\\python.exe",
    });
  });

  it("drops invalid conversation runtime override paths", () => {
    const settings = normalizeStoredSettings({
      conversationRuntimeOverrides: {
        bash: "x".repeat(4097),
        node: 42,
        python: "",
      },
    });

    expect(settings.conversationRuntimeOverrides).toEqual(defaultSettings.conversationRuntimeOverrides);
  });

  it("preserves a configured database backup directory", () => {
    const settings = normalizeStoredSettings({
      dataBackup: {
        customDirectory: "  /Volumes/Asset Backups  ",
      },
    });

    expect(settings.dataBackup.customDirectory).toBe("/Volumes/Asset Backups");
  });

  it("drops invalid database backup directory values", () => {
    const settings = normalizeStoredSettings({
      dataBackup: {
        customDirectory: "x".repeat(4097),
      },
    });

    expect(settings.dataBackup.customDirectory).toBe("");
  });

  it("preserves custom page-level typography overrides", () => {
    const contentFontFamily = createFontFamilySetting("custom", "Inter Variable");
    const codeFontFamily = createFontFamilySetting("custom", "Maple Mono NF CN");
    const sessionBrowserFontFamily = createFontFamilySetting("custom", "LXGW WenKai");
    const settings = normalizeStoredSettings({
      typography: {
        codeFontFamily,
        contentFontFamily,
        contentFontSize: 16,
        interfaceFontFamily: createFontFamilySetting("custom", "SF Pro Display"),
      },
      conversations: {
        contentFontFamily: createFontFamilySetting("custom", "Atkinson Hyperlegible"),
        contentFontSize: 15,
        resultPreviewLineLimit: 12,
        sessionBrowserFontFamily,
        sessionBrowserFontSize: 12,
        sessionToolbarCompact: false,
      },
    });

    expect(settings.typography.codeFontFamily).toEqual(codeFontFamily);
    expect(settings.typography.contentFontFamily).toEqual(contentFontFamily);
    expect(settings.typography.contentFontSize).toBe(16);
    expect(settings.typography.interfaceFontFamily).toEqual(createFontFamilySetting("custom", "SF Pro Display"));
    expect(settings.conversations.contentFontFamily).toEqual(createFontFamilySetting("custom", "Atkinson Hyperlegible"));
    expect(settings.conversations.contentFontSize).toBe(15);
    expect(settings.conversations.resultPreviewLineLimit).toBe(12);
    expect(settings.conversations.sessionBrowserFontFamily).toEqual(sessionBrowserFontFamily);
    expect(settings.conversations.sessionBrowserFontSize).toBe(12);
    expect(settings.conversations.sessionToolbarCompact).toBe(false);
  });

  it("normalizes command result preview line limits", () => {
    expect(normalizeStoredSettings({
      conversations: { resultPreviewLineLimit: 12 },
    }).conversations.resultPreviewLineLimit).toBe(12);
    expect(normalizeStoredSettings({
      conversations: { resultPreviewLineLimit: 2 },
    }).conversations.resultPreviewLineLimit).toBe(RESULT_PREVIEW_LINE_LIMIT_MIN);
    expect(normalizeStoredSettings({
      conversations: { resultPreviewLineLimit: 2000 },
    }).conversations.resultPreviewLineLimit).toBe(RESULT_PREVIEW_LINE_LIMIT_MAX);
  });

  it("migrates legacy font tokens to single editable font names", () => {
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

    expect(settings.typography.codeFontFamily).toEqual(createFontFamilySetting("mono"));
    expect(settings.typography.contentFontFamily).toEqual(createFontFamilySetting("serif"));
    expect(settings.typography.interfaceFontFamily).toEqual(createFontFamilySetting("geist"));
    expect(settings.conversations.contentFontFamily).toEqual(createFontFamilySetting("system"));
    expect(settings.conversations.sessionBrowserFontFamily).toEqual(createFontFamilySetting("mono"));
  });

  it("migrates legacy CSS font-family stacks to the primary font name", () => {
    const settings = normalizeStoredSettings({
      typography: {
        codeFontFamily: fontFamilyCss.mono,
        contentFontFamily: '"Inter Variable", ui-sans-serif, system-ui, sans-serif',
        interfaceFontFamily: fontFamilyCss.geist,
      },
      conversations: {
        sessionBrowserFontFamily: '"LXGW WenKai", Georgia, serif',
      },
    });

    expect(settings.typography.codeFontFamily).toEqual(createFontFamilySetting("mono"));
    expect(settings.typography.contentFontFamily).toEqual(createFontFamilySetting("custom", "Inter Variable"));
    expect(settings.typography.interfaceFontFamily).toEqual(createFontFamilySetting("geist"));
    expect(settings.conversations.sessionBrowserFontFamily).toEqual(createFontFamilySetting("custom", "LXGW WenKai"));
  });

  it("resolves single font names to CSS font-family stacks at render time", () => {
    expect(resolveFontFamilyCss(createFontFamilySetting("custom", "Maple Mono NF CN"), "mono")).toBe(
      '"Maple Mono NF CN", "JetBrains Mono", "SFMono-Regular", Consolas, monospace',
    );
    expect(resolveFontFamilyCss(createFontFamilySetting("geist"), "sans")).toBe(fontFamilyCss.geist);
  });

  it("preserves custom mode even when the custom font name matches a preset font", () => {
    const settings = normalizeStoredSettings({
      typography: {
        codeFontFamily: createFontFamilySetting("custom", "JetBrains Mono"),
      },
    });

    expect(settings.typography.codeFontFamily).toEqual(createFontFamilySetting("custom", "JetBrains Mono"));
  });

  it("normalizes invalid typography values", () => {
    const settings = normalizeStoredSettings({
      typography: {
        baseFontSize: 99,
        codeFontFamily: createFontFamilySetting("custom", "Arial; color: red"),
      },
      conversations: {
        contentFontSize: 2,
        sessionBrowserFontFamily: createFontFamilySetting("custom", "Bad { font-family: serif }"),
      },
    });

    expect(settings.typography.baseFontSize).toBe(FONT_SIZE_MAX);
    expect(settings.typography.codeFontFamily).toBe(defaultSettings.typography.codeFontFamily);
    expect(settings.conversations.contentFontSize).toBe(FONT_SIZE_MIN);
    expect(settings.conversations.sessionBrowserFontFamily).toBe(defaultSettings.typography.contentFontFamily);
  });
});
