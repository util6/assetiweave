// @vitest-environment jsdom

import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { navigationModel } from "../../router/menu";
import { defaultSettings } from "../../store/settings/settingsSchema";
import { GlobalSettingsDialog } from "./GlobalSettingsDialog";

const startSyncMock = vi.hoisted(() => vi.fn());
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
      updateSetting: vi.fn(),
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
  listConversationTranslationModels: vi.fn().mockResolvedValue([]),
  testConversationTranslationConnection: vi.fn(),
}));

describe("GlobalSettingsDialog full conversation sync", () => {
  beforeEach(() => {
    conversationSyncState.tasks = [];
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
});
