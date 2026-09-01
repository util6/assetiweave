import { describe, expect, it } from "vitest";
import {
  COLUMN_MIN_WIDTH_MAX,
  COLUMN_MIN_WIDTH_MIN,
  DEFAULT_COLUMN_MIN_WIDTH,
  DEFAULT_CONVERSATION_FULL_SYNC_ON_STARTUP,
  DEFAULT_PROMPT_OPTIMIZATION_PROMPT_TEMPLATE,
  FONT_SIZE_MAX,
  FONT_SIZE_MIN,
  RESULT_PREVIEW_LINE_LIMIT_MAX,
  RESULT_PREVIEW_LINE_LIMIT_MIN,
  createFontFamilySetting,
  defaultSettings,
  DEFAULT_CONVERSATION_TRANSLATION_PROMPT_TEMPLATE,
  fontFamilyCss,
  assignAgentToAction,
  assignModelToAgentActions,
  normalizeStoredSettings,
  resolveAgentCapability,
  resolveFontFamilyCss,
} from "./settingsSchema";

describe("AppSettingsProvider", () => {
  it("enables startup full conversation sync for older stored settings", () => {
    expect(defaultSettings.conversations.autoFullSyncOnStartup).toBe(DEFAULT_CONVERSATION_FULL_SYNC_ON_STARTUP);
    expect(normalizeStoredSettings({}).conversations.autoFullSyncOnStartup).toBe(true);
  });

  it("preserves an explicit startup full conversation sync preference", () => {
    expect(normalizeStoredSettings({
      conversations: { autoFullSyncOnStartup: false },
    }).conversations.autoFullSyncOnStartup).toBe(false);
  });

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
    expect(settings.conversationTranslation).toEqual(defaultSettings.conversationTranslation);
    expect(settings.dataBackup).toEqual(defaultSettings.dataBackup);
    expect(settings.conversationRuntimeOverrides).toEqual(defaultSettings.conversationRuntimeOverrides);
  });

  it("preserves dynamic and unregistered conversation Card color overrides", () => {
    const settings = normalizeStoredSettings({
      conversations: {
        contentCardColors: {
          "claude-code.reasoning": "#123abc",
          "history.custom-trace": "#fedcba",
          "Invalid Kind": "#ffffff",
        },
      },
    });

    expect(settings.conversations.contentCardColors["claude-code.reasoning"]).toBe("#123abc");
    expect(settings.conversations.contentCardColors["history.custom-trace"]).toBe("#fedcba");
    expect(settings.conversations.contentCardColors["Invalid Kind"]).toBeUndefined();
  });

  it("preserves configured conversation runtime override paths", () => {
    const settings = normalizeStoredSettings({
      conversationRuntimeOverrides: {
        bash: "  /opt/homebrew/bin/bash  ",
        node: "~/.local/bin/node",
        python: "C:\\Python312\\python.exe",
      },
    });

    expect(settings.conversationRuntimeOverrides).toEqual({
      bash: "/opt/homebrew/bin/bash",
      node: "~/.local/bin/node",
      python: "C:\\Python312\\python.exe",
    });
  });

  it("drops invalid conversation runtime override paths", () => {
    const settings = normalizeStoredSettings({
      conversationRuntimeOverrides: {
        bash: "x".repeat(4097),
        node: "node",
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
    expect(settings.conversationTranslation.targetLanguage).toBe(defaultSettings.conversationTranslation.targetLanguage);
  });

  it("migrates and normalizes conversation translation settings", () => {
    expect(normalizeStoredSettings({
      conversations: { translationTargetLanguage: "  Spanish (Latin America)  " },
    }).conversationTranslation.targetLanguage).toBe("Spanish (Latin America)");
    const normalized = normalizeStoredSettings({
      conversationTranslation: {
        cli: "gemini",
        model: "gemini-2.5-pro",
        promptTemplate: "Translate into {targetLanguage}: {content}",
        provider: "cli",
        targetLanguage: "French\n\nCanadian",
      },
    });
    expect(normalized.agentAssignments["translation.card"]).toEqual({
      agentId: "gemini",
      modelId: "gemini-2.5-pro",
    });
    expect(normalized.conversationTranslation).toEqual({
      promptTemplate: "Translate into {targetLanguage}: {content}",
      provider: "cli",
      targetLanguage: "French Canadian",
    });
    expect(normalizeStoredSettings({
      conversationTranslation: {
        cli: "custom",
        model: "x".repeat(200),
        promptTemplate: "",
        provider: "google",
        targetLanguage: "zh-CN",
      },
    }).conversationTranslation).toEqual({
      ...defaultSettings.conversationTranslation,
      provider: "google",
    });
    expect(DEFAULT_CONVERSATION_TRANSLATION_PROMPT_TEMPLATE).toContain("{targetLanguage}");
    expect(DEFAULT_CONVERSATION_TRANSLATION_PROMPT_TEMPLATE).toContain("{content}");
  });

  it("migrates and normalizes prompt optimization settings", () => {
    const legacyDefaultPrompt = [
      "You are an expert prompt editor.",
      "Rewrite the content into a clearer, more actionable prompt.",
      "Keep the user's intent, constraints, domain terms, variables, Markdown, and code fences.",
      "Improve structure, remove ambiguity, and make the requested outcome explicit.",
      "Target working language: {targetLanguage}.",
      "Return only the optimized prompt. Do not add commentary.",
      "",
      "<content>",
      "{content}",
      "</content>",
    ].join("\n");

    const customized = normalizeStoredSettings({
      promptOptimization: {
        promptTemplate: "  Optimize this request for a technical audience.\r\n\r\n{content}  ",
      },
    });

    expect(customized.promptOptimization).toEqual({
      promptTemplate: "Optimize this request for a technical audience.\n\n{content}",
    });
    expect(normalizeStoredSettings({
      promptOptimization: { promptTemplate: "" },
    }).promptOptimization).toEqual(defaultSettings.promptOptimization);
    expect(normalizeStoredSettings({
      promptOptimization: { promptTemplate: legacyDefaultPrompt },
    }).promptOptimization).toEqual(defaultSettings.promptOptimization);
    expect(DEFAULT_PROMPT_OPTIMIZATION_PROMPT_TEMPLATE).toContain("{content}");
    expect(DEFAULT_PROMPT_OPTIMIZATION_PROMPT_TEMPLATE).not.toContain("{targetLanguage}");
  });

  it("migrates legacy translation runtime settings into canonical action assignments", () => {
    const settings = normalizeStoredSettings({
      conversationTranslation: {
        cli: "gemini",
        model: "  gemini-2.5-pro  ",
        provider: "cli",
        targetLanguage: "English",
      },
    });

    expect(settings.agentAssignments["translation.card"]).toEqual({
      agentId: "gemini",
      modelId: "gemini-2.5-pro",
    });
    expect(settings.conversationTranslation).toEqual({
      promptTemplate: defaultSettings.conversationTranslation.promptTemplate,
      provider: "cli",
      targetLanguage: "English",
    });
  });

  it("keeps canonical action assignments explicitly unassigned", () => {
    const settings = normalizeStoredSettings({
      agentAssignments: {
        "translation.card": { agentId: "opencode", modelId: "model/a" },
      },
    });

    expect(settings.agentAssignments).toEqual({
      "translation.card": { agentId: "opencode", modelId: "model/a" },
    });
    expect(resolveAgentCapability(settings, "memory")).toEqual({
      agentId: "",
      model: "",
    });
  });

  it("migrates service Agent assignments from the legacy runtime and keeps models per Agent", () => {
    const settings = normalizeStoredSettings({
      aiRuntime: { cli: "gemini", model: "gemini-2.5-pro" },
      agentModels: { codex: "openai/gpt-5-codex" },
      agentCapabilityAssignments: { memory: "codex" },
    });

    expect(settings.agentAssignments).toEqual({
      "translation.card": { agentId: "gemini", modelId: "gemini-2.5-pro" },
      "memory.extraction": { agentId: "codex", modelId: "openai/gpt-5-codex" },
      "memory.project": { agentId: "codex", modelId: "openai/gpt-5-codex" },
      "memory.global": { agentId: "codex", modelId: "openai/gpt-5-codex" },
      "memory.recall": { agentId: "codex", modelId: "openai/gpt-5-codex" },
      "prompt.optimization": { agentId: "gemini", modelId: "gemini-2.5-pro" },
    });
    expect(settings).not.toHaveProperty("agentCapabilityAssignments");
    expect(settings).not.toHaveProperty("agentModels");
    expect(settings).not.toHaveProperty("aiRuntime");
  });

  it("updates canonical action models when the Agent model changes", () => {
    const assignments = assignModelToAgentActions({
      "translation.card": { agentId: "opencode", modelId: "opencode/expired" },
      "memory.extraction": { agentId: "opencode", modelId: null },
      "prompt.optimization": { agentId: "opencode", modelId: "opencode/expired" },
    }, "opencode", "opencode/hy3-free");

    expect(assignments["translation.card"]?.modelId).toBe("opencode/hy3-free");
    expect(assignments["memory.extraction"]?.modelId).toBe("opencode/hy3-free");
    expect(assignments["prompt.optimization"]?.modelId).toBe("opencode/hy3-free");
  });

  it("replaces an action Agent and its model atomically", () => {
    const assignments = assignAgentToAction({
      "translation.card": { agentId: "opencode", modelId: "opencode/expired" },
      "memory.extraction": { agentId: "opencode", modelId: null },
      "prompt.optimization": { agentId: "opencode", modelId: null },
    }, "translation.card", "codex", "openai/gpt-5");

    expect(assignments["translation.card"]).toEqual({
      agentId: "codex",
      modelId: "openai/gpt-5",
    });
  });

  it("resolves execution from the canonical action assignment", () => {
    const settings = normalizeStoredSettings({
      agentAssignments: {
        "translation.card": { agentId: "opencode", modelId: "opencode/hy3-free" },
      },
      agentModels: { opencode: "opencode/expired" },
    });

    expect(resolveAgentCapability(settings, "cardTranslation")).toEqual({
      agentId: "opencode",
      model: "opencode/hy3-free",
    });
  });

  it("normalizes Memory generation, usage, and exclusion settings", () => {
    expect(normalizeStoredSettings({}).memory).toEqual({
      generationEnabled: true,
      usageEnabled: true,
      excludedSessionIds: [],
      excludedSourceIds: [],
    });
    expect(normalizeStoredSettings({
      memory: {
        generationEnabled: false,
        usageEnabled: false,
        excludedSessionIds: [" session-1 ", "session-1"],
        excludedSourceIds: ["source-1"],
      },
    }).memory).toEqual({
      generationEnabled: false,
      usageEnabled: false,
      excludedSessionIds: ["session-1"],
      excludedSourceIds: ["source-1"],
    });
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
        interfaceFontFamily: "jetbrains",
      },
      conversations: {
        contentFontFamily: "system",
        sessionBrowserFontFamily: "mono",
      },
    });

    expect(settings.typography.codeFontFamily).toEqual(createFontFamilySetting("mono"));
    expect(settings.typography.contentFontFamily).toEqual(createFontFamilySetting("serif"));
    expect(settings.typography.interfaceFontFamily).toEqual(createFontFamilySetting("jetbrains"));
    expect(settings.conversations.contentFontFamily).toEqual(createFontFamilySetting("system"));
    expect(settings.conversations.sessionBrowserFontFamily).toEqual(createFontFamilySetting("mono"));
  });

  it("migrates legacy CSS font-family stacks to the primary font name", () => {
    const settings = normalizeStoredSettings({
      typography: {
        codeFontFamily: fontFamilyCss.mono,
        contentFontFamily: '"Inter Variable", ui-sans-serif, system-ui, sans-serif',
        interfaceFontFamily: fontFamilyCss.jetbrains,
      },
      conversations: {
        sessionBrowserFontFamily: '"LXGW WenKai", Georgia, serif',
      },
    });

    expect(settings.typography.codeFontFamily).toEqual(createFontFamilySetting("mono"));
    expect(settings.typography.contentFontFamily).toEqual(createFontFamilySetting("custom", "Inter Variable"));
    expect(settings.typography.interfaceFontFamily).toEqual(createFontFamilySetting("jetbrains"));
    expect(settings.conversations.sessionBrowserFontFamily).toEqual(createFontFamilySetting("custom", "LXGW WenKai"));
  });

  it("resolves single font names to CSS font-family stacks at render time", () => {
    expect(resolveFontFamilyCss(createFontFamilySetting("custom", "Maple Mono NF CN"), "mono")).toBe(
      '"Maple Mono NF CN", "JetBrains Mono", "SFMono-Regular", Consolas, monospace',
    );
    expect(resolveFontFamilyCss(createFontFamilySetting("jetbrains"), "sans")).toBe(fontFamilyCss.jetbrains);
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
