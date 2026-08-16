// @vitest-environment jsdom

import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { appShortcutIconCatalog } from "../../config/appShortcutIcons";
import { fallbackAppShortcuts } from "../../mock/catalog";
import { navigationModel } from "../../router/menu";
import { defaultSettings } from "../../store/settings/settingsSchema";
import { GlobalSettingsDialog } from "./GlobalSettingsDialog";

const startSyncMock = vi.hoisted(() => vi.fn());
const updateSettingMock = vi.hoisted(() => vi.fn());
const conversationSyncState = vi.hoisted(() => ({ tasks: [] as Array<Record<string, unknown>> }));

vi.mock("../../app/backgroundTasks/ConversationSyncProvider", () => ({
  useConversationSync: () => ({
    startSync: startSyncMock,
    task: null,
    taskFor: () => null,
    tasks: conversationSyncState.tasks,
  }),
}));

vi.mock("../../i18n/I18nProvider", () => ({
  useI18n: () => ({ locale: "en", setLocale: vi.fn(), t: (key: string) => key }),
}));

vi.mock("../../store/settings/AppSettingsProvider", async (importOriginal) => {
  const actual = await importOriginal<typeof import("../../store/settings/AppSettingsProvider")>();
  return {
    ...actual,
    useAppSettings: () => ({
      resetSettings: vi.fn(),
      settings: defaultSettings,
      storageInfo: {
        configDir: "/tmp/config",
        configPath: "/tmp/config/settings.json",
        conversationAdapterDir: "/tmp/adapters",
        defaultDataBackupDir: "/tmp/backups",
      },
      updateSetting: updateSettingMock,
    }),
  };
});

vi.mock("../../services/catalog", () => ({
  getSkillBackupSettings: vi.fn().mockResolvedValue(null),
  revealPath: vi.fn(),
  selectTargetDirectory: vi.fn(),
}));

vi.mock("../../services/cliTools", () => ({
  getCliToolsStatus: vi.fn().mockResolvedValue(null),
  installCliTools: vi.fn(),
}));

vi.mock("../../services/conversations", () => ({
  listConversationAdapterRuntimeStatuses: vi.fn().mockResolvedValue([]),
}));

vi.mock("../../services/cardTranslation", () => ({
  checkOpencodeTranslationAvailability: vi.fn().mockResolvedValue({
    available: true,
    error: null,
    version: "opencode 1.0.0",
  }),
  listConversationTranslationModels: vi.fn().mockResolvedValue([]),
  testConversationTranslationConnection: vi.fn(),
}));

describe("GlobalSettingsDialog", () => {
  beforeEach(() => {
    conversationSyncState.tasks = [];
    updateSettingMock.mockReset();
    startSyncMock.mockReset().mockResolvedValue({
      id: "sync-full",
      status: "running",
      source_id: null,
      adapter_id: null,
      record_kind: null,
      mode: "full",
      dry_run: false,
      started_at: "2026-07-27T00:00:00Z",
      finished_at: null,
      result: null,
      error: null,
    });
  });

  afterEach(() => cleanup());

  it("exposes the Agent settings panel through the settings navigation", async () => {
    render(
      <GlobalSettingsDialog
        appShortcuts={[]}
        initialPanel="general.agents"
        navigationModel={navigationModel}
        onAppShortcutsChange={vi.fn()}
        onClose={vi.fn()}
        onNavigationModelChange={vi.fn()}
        open
      />,
    );

    expect(screen.getByRole("heading", { name: "settings.agents.title" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "settings.agents.addCustom" })).toBeTruthy();
    await waitFor(() => expect(screen.getByRole("heading", { name: "OpenCode" })).toBeTruthy());
  });

  it("renders every built-in APP icon as an editable row in the shared APP settings list", () => {
    render(
      <GlobalSettingsDialog
        appShortcuts={fallbackAppShortcuts}
        initialPanel="workspace.shortcuts"
        navigationModel={navigationModel}
        onAppShortcutsChange={vi.fn()}
        onClose={vi.fn()}
        onNavigationModelChange={vi.fn()}
        open
      />,
    );

    expect(screen.getAllByLabelText("settings.shortcuts.color")).toHaveLength(appShortcutIconCatalog.length);
    expect(screen.queryByRole("list", { name: "settings.shortcuts.appIcon" })).toBeNull();
    for (const shortcut of fallbackAppShortcuts) {
      expect(screen.getByText(shortcut.profileName)).toBeTruthy();
    }
  });

  it("keeps the Memory panel focused on Memory controls", () => {
    render(
      <GlobalSettingsDialog
        appShortcuts={[]}
        initialPanel="general.memory"
        navigationModel={navigationModel}
        onAppShortcutsChange={vi.fn()}
        onClose={vi.fn()}
        onNavigationModelChange={vi.fn()}
        open
      />,
    );

    expect(screen.getByRole("heading", { name: "settings.section.memory" })).toBeTruthy();
    expect(screen.queryByText("settings.conversation.translationCli")).toBeNull();
    expect(screen.queryByText("settings.conversation.translationModel")).toBeNull();
    expect(screen.queryByText("settings.conversation.translationConnection")).toBeNull();
    expect(screen.queryByText("settings.ai.executionBoundary")).toBeNull();
    expect(screen.getByText("settings.ai.autoDream")).toBeTruthy();
  });

  it("assigns a service Agent from the brief capability picker", async () => {
    render(
      <GlobalSettingsDialog
        appShortcuts={[]}
        initialPanel="general.memory"
        navigationModel={navigationModel}
        onAppShortcutsChange={vi.fn()}
        onClose={vi.fn()}
        onNavigationModelChange={vi.fn()}
        open
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: /OpenCode/ }));
    expect(screen.getByRole("heading", { name: "settings.agentCapabilities.dialogTitle" })).toBeTruthy();
    expect(screen.getByText("settings.agentCapabilities.selectedLabel")).toBeTruthy();
    expect(screen.getAllByText("settings.agentCapabilities.usingDefaultModel").length).toBeGreaterThan(0);
    expect(screen.getByRole("list", { name: "settings.agentCapabilities.dialogTitle" })).toBeTruthy();

    const geminiOptions = screen.getAllByRole("button", { name: /Gemini CLI/ });
    fireEvent.click(geminiOptions[0]);

    expect(updateSettingMock).toHaveBeenCalledWith("agentCapabilityAssignments", {
      ...defaultSettings.agentCapabilityAssignments,
      memory: "gemini",
    });
  });

  it("opens the capability list as a separate modal above the settings surface", () => {
    render(
      <GlobalSettingsDialog
        appShortcuts={[]}
        initialPanel="general.memory"
        navigationModel={navigationModel}
        onAppShortcutsChange={vi.fn()}
        onClose={vi.fn()}
        onNavigationModelChange={vi.fn()}
        open
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: /OpenCode/ }));

    const dialog = screen.getByRole("dialog");
    expect(within(dialog).getByRole("list", { name: "settings.agentCapabilities.dialogTitle" })).toBeTruthy();
    expect(within(dialog).getByRole("button", { name: "common.close" })).toBeTruthy();
  });

  it("keeps settings navigation and modal content in independent scroll containers", () => {
    render(
      <GlobalSettingsDialog
        appShortcuts={[]}
        initialPanel="general.memory"
        navigationModel={navigationModel}
        onAppShortcutsChange={vi.fn()}
        onClose={vi.fn()}
        onNavigationModelChange={vi.fn()}
        open
      />,
    );

    const navigation = screen.getByRole("navigation", { name: "settings.navAria" });
    expect(navigation.className).toContain("overflow-y-auto");
    expect(navigation.className).toContain("min-h-0");

    fireEvent.click(screen.getByRole("button", { name: /OpenCode/ }));
    const dialogList = screen.getByRole("list", { name: "settings.agentCapabilities.dialogTitle" });
    expect(dialogList.className).not.toContain("overflow-y-auto");
    expect(dialogList.parentElement?.parentElement?.className).toContain("overflow-y-auto");
  });

  it("jumps from the capability picker to the focused Agent settings row", () => {
    render(
      <GlobalSettingsDialog
        appShortcuts={[]}
        initialPanel="general.memory"
        navigationModel={navigationModel}
        onAppShortcutsChange={vi.fn()}
        onClose={vi.fn()}
        onNavigationModelChange={vi.fn()}
        open
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: /OpenCode/ }));
    fireEvent.click(screen.getAllByRole("button", { name: /settings\.agentCapabilities\.openAgentSettings Codex CLI/ })[0]);

    expect(screen.getByRole("heading", { name: "settings.agents.title" })).toBeTruthy();
    expect(screen.getByRole("heading", { name: "Codex CLI" })).toBeTruthy();
  });

  it.each([
    ["conversations.translation", "cardTranslation", "codex"],
    ["general.promptOptimization", "promptOptimization", "claude"],
  ] as const)("stores the Agent assignment for %s", (initialPanel, serviceId, agentId) => {
    render(
      <GlobalSettingsDialog
        appShortcuts={[]}
        initialPanel={initialPanel}
        navigationModel={navigationModel}
        onAppShortcutsChange={vi.fn()}
        onClose={vi.fn()}
        onNavigationModelChange={vi.fn()}
        open
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: /OpenCode/ }));
    fireEvent.click(screen.getAllByRole("button", { name: new RegExp(agentId === "codex" ? "Codex CLI" : "Claude Code") })[0]);

    expect(updateSettingMock).toHaveBeenCalledWith("agentCapabilityAssignments", {
      ...defaultSettings.agentCapabilityAssignments,
      [serviceId]: agentId,
    });
  });

  it("shows and stores the editable prompt optimization system prompt", () => {
    render(
      <GlobalSettingsDialog
        appShortcuts={[]}
        initialPanel="general.promptOptimization"
        navigationModel={navigationModel}
        onAppShortcutsChange={vi.fn()}
        onClose={vi.fn()}
        onNavigationModelChange={vi.fn()}
        open
      />,
    );

    const systemPrompt = screen.getByRole("textbox", {
      name: "settings.promptOptimization.systemPrompt",
    });
    expect((systemPrompt as HTMLTextAreaElement).value).toBe(defaultSettings.promptOptimization.promptTemplate);

    fireEvent.change(systemPrompt, {
      target: { value: "Optimize the request for an implementation plan.\n{content}" },
    });

    expect(updateSettingMock).toHaveBeenCalledWith("promptOptimization", {
      promptTemplate: "Optimize the request for an implementation plan.\n{content}",
    });
  });

  it("starts an all-record full reparse only after explicit confirmation", async () => {
    render(
      <GlobalSettingsDialog
        appShortcuts={[]}
        initialPanel="conversations.sessions"
        navigationModel={navigationModel}
        onAppShortcutsChange={vi.fn()}
        onClose={vi.fn()}
        onNavigationModelChange={vi.fn()}
        open
      />,
    );

    fireEvent.click(screen.getByRole("button", { name: "settings.conversation.fullSyncAction" }));
    expect(startSyncMock).not.toHaveBeenCalled();

    fireEvent.click(screen.getByRole("button", { name: "settings.conversation.fullSyncConfirmAction" }));

    await waitFor(() => expect(startSyncMock).toHaveBeenCalledWith({
      dry_run: false,
      mode: "full",
      record_kind: null,
    }));
  });

  it("shows source-level progress for a running full reparse", () => {
    conversationSyncState.tasks = [{
      id: "sync-full",
      status: "running",
      source_id: null,
      adapter_id: null,
      record_kind: null,
      mode: "full",
      dry_run: false,
      started_at: "2026-07-28T00:00:00Z",
      finished_at: null,
      result: null,
      error: null,
      progress: {
        phase: "syncing",
        completed_source_count: 1,
        total_source_count: 3,
        current_source_name: "Gemini Web",
      },
    }];

    render(
      <GlobalSettingsDialog
        appShortcuts={[]}
        initialPanel="conversations.sessions"
        navigationModel={navigationModel}
        onAppShortcutsChange={vi.fn()}
        onClose={vi.fn()}
        onNavigationModelChange={vi.fn()}
        open
      />,
    );

    const progress = screen.getByRole("progressbar", {
      name: "settings.conversation.fullSyncProgressLabel",
    });
    expect(progress.getAttribute("aria-valuenow")).toBe("1");
    expect(progress.getAttribute("aria-valuemax")).toBe("3");
    expect(screen.getByText("Gemini Web")).toBeTruthy();
    const runningButton = screen.getByRole("button", {
      name: "settings.conversation.fullSyncButtonRunningWithProgress",
    });
    expect(runningButton.className).toContain("disabled:opacity-100");
    expect(runningButton.querySelector("svg")?.getAttribute("class")).toContain("motion-safe:animate-spin");
  });

  it("updates the startup full sync preference from the conversation settings", () => {
    render(
      <GlobalSettingsDialog
        appShortcuts={[]}
        initialPanel="conversations.sessions"
        navigationModel={navigationModel}
        onAppShortcutsChange={vi.fn()}
        onClose={vi.fn()}
        onNavigationModelChange={vi.fn()}
        open
      />,
    );

    fireEvent.click(screen.getByRole("switch", {
      name: "settings.conversation.autoFullSyncOnStartup",
    }));

    expect(updateSettingMock).toHaveBeenCalledWith("conversations", {
      ...defaultSettings.conversations,
      autoFullSyncOnStartup: false,
    });
  });
});
