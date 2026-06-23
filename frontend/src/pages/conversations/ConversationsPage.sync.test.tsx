/* @vitest-environment jsdom */

import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { I18nProvider } from "../../i18n/I18nProvider";
import { defaultSettings } from "../../store/settings/settingsSchema";
import { ConversationsPage } from "./ConversationsPage";

const startSyncMock = vi.hoisted(() => vi.fn());
const listConversationAdaptersMock = vi.hoisted(() => vi.fn());
const listConversationSessionsMock = vi.hoisted(() => vi.fn());
const listWebRecordSessionsMock = vi.hoisted(() => vi.fn());

vi.mock("../../app/backgroundTasks/ConversationSyncProvider", () => ({
  useConversationSync: () => ({
    startSync: startSyncMock,
    task: null,
  }),
}));

vi.mock("../../store/settings/AppSettingsProvider", async () => {
  const actual = await vi.importActual<typeof import("../../store/settings/AppSettingsProvider")>(
    "../../store/settings/AppSettingsProvider",
  );
  return {
    ...actual,
    useAppSettings: () => ({
      resetSettings: vi.fn(),
      settings: defaultSettings,
      settingsError: null,
      settingsLoaded: true,
      storageInfo: {},
      updateSetting: vi.fn(),
    }),
  };
});

vi.mock("../../services/conversations", async () => {
  const actual = await vi.importActual<typeof import("../../services/conversations")>(
    "../../services/conversations",
  );
  return {
    ...actual,
    listConversationAdapters: listConversationAdaptersMock,
    listConversationSessions: listConversationSessionsMock,
    listWebRecordSessions: listWebRecordSessionsMock,
  };
});

describe("ConversationsPage sync scope", () => {
  beforeEach(() => {
    window.scrollTo = vi.fn();
    vi.stubGlobal("ResizeObserver", class {
      disconnect() {}
      observe() {}
      unobserve() {}
    });
    startSyncMock.mockReset().mockResolvedValue({
      adapter_id: null,
      dry_run: false,
      error: null,
      finished_at: null,
      id: "sync-1",
      result: null,
      source_id: null,
      started_at: "2026-06-15T00:00:00Z",
      status: "running",
    });
    listConversationAdaptersMock.mockReset().mockResolvedValue([]);
    listConversationSessionsMock.mockReset().mockResolvedValue([]);
    listWebRecordSessionsMock.mockReset().mockResolvedValue([]);
  });

  afterEach(() => {
    cleanup();
    vi.unstubAllGlobals();
    vi.clearAllMocks();
  });

  it("syncs only conversation sessions from the conversations page", async () => {
    renderConversationsPage("session");

    fireEvent.click(screen.getByRole("button", { name: "Sync" }));

    await waitFor(() =>
      expect(startSyncMock).toHaveBeenCalledWith({
        dry_run: false,
        record_kind: "session",
        source_id: null,
      }),
    );
  });

  it("syncs only web records from the web records page", async () => {
    renderConversationsPage("web");

    fireEvent.click(screen.getByRole("button", { name: "Sync" }));

    await waitFor(() =>
      expect(startSyncMock).toHaveBeenCalledWith({
        dry_run: false,
        record_kind: "web",
        source_id: null,
      }),
    );
  });
});

function renderConversationsPage(recordKind: "session" | "web") {
  render(
    <I18nProvider>
      <ConversationsPage
        appShortcuts={[]}
        onManualOpen={vi.fn()}
        onNotifyError={vi.fn()}
        onOpenSettings={vi.fn()}
        recordKind={recordKind}
      />
    </I18nProvider>,
  );
}
