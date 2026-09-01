// @vitest-environment jsdom

import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ConversationSyncProvider, useConversationSync } from "./ConversationSyncProvider";

const subscribeConversationSyncTasksMock = vi.hoisted(() => vi.fn());
const listConversationSyncTasksMock = vi.hoisted(() => vi.fn());
const syncConversationsMock = vi.hoisted(() => vi.fn());
const cancelConversationSyncMock = vi.hoisted(() => vi.fn());

vi.mock("../../services/conversations", () => ({
  listConversationSyncTasks: listConversationSyncTasksMock,
  subscribeConversationSyncTasks: subscribeConversationSyncTasksMock,
  syncConversations: syncConversationsMock,
  cancelConversationSync: cancelConversationSyncMock,
}));

describe("ConversationSyncProvider", () => {
  beforeEach(() => {
    subscribeConversationSyncTasksMock.mockReset().mockResolvedValue(vi.fn());
    listConversationSyncTasksMock.mockReset().mockResolvedValue([]);
    syncConversationsMock.mockReset();
    cancelConversationSyncMock.mockReset();
  });

  afterEach(() => {
    cleanup();
    vi.useRealTimers();
    vi.clearAllMocks();
  });

  it("keeps the rest of the app interactive while receiving background sync events", async () => {
    const runningTask = {
      id: "sync-1",
      status: "running",
      source_id: null,
      adapter_id: null,
      dry_run: false,
      started_at: "2026-06-15T00:00:00Z",
      finished_at: null,
      result: null,
      error: null,
    } as const;
    syncConversationsMock.mockResolvedValue(runningTask);
    let syncListener: ((snapshot: unknown) => void) | undefined;
    subscribeConversationSyncTasksMock.mockImplementation(
      async (listener: (snapshot: unknown) => void) => {
        syncListener = listener;
        return vi.fn();
      },
    );

    render(
      <ConversationSyncProvider>
        <ProviderHarness />
      </ConversationSyncProvider>,
    );

    fireEvent.click(screen.getByRole("button", { name: "Start sync" }));
    await act(async () => {});
    expect(
      (screen.getByRole("button", { name: "Other feature" }) as HTMLButtonElement).disabled,
    ).toBe(false);
    expect(screen.getByTestId("sync-status").textContent).toBe("running");

    await act(async () => {
      syncListener?.({
        ...runningTask,
        status: "completed",
        finished_at: "2026-06-15T00:00:05Z",
        result: { results: [] },
      });
    });
    expect(screen.getByTestId("sync-status").textContent).toBe("completed");
  });

  it("cancels a running sync through the shared task runtime", async () => {
    const runningTask = {
      id: "sync-cancel",
      status: "running",
      source_id: null,
      adapter_id: null,
      record_kind: "session",
      dry_run: false,
      started_at: "2026-06-15T00:00:00Z",
      finished_at: null,
      result: null,
      error: null,
    } as const;
    syncConversationsMock.mockResolvedValue(runningTask);
    cancelConversationSyncMock.mockResolvedValue({ ...runningTask, status: "cancelling" });

    render(
      <ConversationSyncProvider>
        <CancelSyncHarness />
      </ConversationSyncProvider>,
    );

    fireEvent.click(screen.getByRole("button", { name: "Start and cancel sync" }));
    await act(async () => {});
    expect(screen.getByTestId("cancel-sync-status").textContent).toBe("cancelling");
    expect(cancelConversationSyncMock).toHaveBeenCalledWith("sync-cancel");
  });

  it("uses status polling as a fallback when a completion event is missed", async () => {
    vi.useFakeTimers();
    const runningTask = {
      id: "sync-1",
      status: "running",
      source_id: null,
      adapter_id: null,
      dry_run: false,
      started_at: "2026-06-15T00:00:00Z",
      finished_at: null,
      result: null,
      error: null,
    } as const;
    syncConversationsMock.mockResolvedValue(runningTask);
    listConversationSyncTasksMock
      .mockResolvedValueOnce([])
      .mockResolvedValueOnce([{
        ...runningTask,
        record_kind: "session",
        status: "completed",
        finished_at: "2026-06-15T00:00:05Z",
        result: { results: [] },
      }]);

    render(
      <ConversationSyncProvider>
        <ProviderHarness />
      </ConversationSyncProvider>,
    );

    fireEvent.click(screen.getByRole("button", { name: "Start sync" }));
    await act(async () => {});
    expect(screen.getByTestId("sync-status").textContent).toBe("running");

    await act(async () => {
      await vi.advanceTimersByTimeAsync(1000);
    });
    expect(screen.getByTestId("sync-status").textContent).toBe("completed");
  });

  it("tracks session and web sync tasks independently", async () => {
    syncConversationsMock.mockImplementation(async (params: { record_kind?: string }) => ({
      id: `sync-${params.record_kind}`,
      status: "running",
      source_id: null,
      adapter_id: null,
      record_kind: params.record_kind,
      dry_run: false,
      started_at: "2026-06-15T00:00:00Z",
      finished_at: null,
      result: null,
      error: null,
    }));

    render(
      <ConversationSyncProvider>
        <IndependentSyncHarness />
      </ConversationSyncProvider>,
    );

    fireEvent.click(screen.getByRole("button", { name: "Start session sync" }));
    fireEvent.click(screen.getByRole("button", { name: "Start web sync" }));
    await act(async () => {});

    expect(screen.getByTestId("session-sync-status").textContent).toBe("running");
    expect(screen.getByTestId("web-sync-status").textContent).toBe("running");
    expect(syncConversationsMock).toHaveBeenCalledTimes(2);
  });

  it("tracks a full all-record sync without treating it as a session-only task", async () => {
    syncConversationsMock.mockResolvedValue({
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

    render(
      <ConversationSyncProvider>
        <FullSyncHarness />
      </ConversationSyncProvider>,
    );

    fireEvent.click(screen.getByRole("button", { name: "Start full sync" }));
    await act(async () => {});

    expect(screen.getByTestId("full-sync-status").textContent).toBe("running");
    expect(screen.getByTestId("session-sync-status").textContent).toBe("running");
    expect(screen.getByTestId("web-sync-status").textContent).toBe("running");
  });
});

function ProviderHarness() {
  const { startSync, task } = useConversationSync();

  return (
    <>
      <button onClick={() => void startSync({ source_id: null, dry_run: false })} type="button">
        Start sync
      </button>
      <button type="button">Other feature</button>
      <output data-testid="sync-status">{task?.status ?? "idle"}</output>
    </>
  );
}

function IndependentSyncHarness() {
  const { startSync, taskFor } = useConversationSync();

  return (
    <>
      <button onClick={() => void startSync({ record_kind: "session" })} type="button">
        Start session sync
      </button>
      <button onClick={() => void startSync({ record_kind: "web" })} type="button">
        Start web sync
      </button>
      <output data-testid="session-sync-status">{taskFor("session")?.status ?? "idle"}</output>
      <output data-testid="web-sync-status">{taskFor("web")?.status ?? "idle"}</output>
    </>
  );
}

function FullSyncHarness() {
  const { startSync, taskFor, tasks } = useConversationSync();
  const fullTask = tasks.find((task) => task.record_kind == null && task.mode === "full");

  return (
    <>
      <button onClick={() => void startSync({ mode: "full", record_kind: null })} type="button">
        Start full sync
      </button>
      <output data-testid="full-sync-status">{fullTask?.status ?? "idle"}</output>
      <output data-testid="session-sync-status">{taskFor("session")?.status ?? "idle"}</output>
      <output data-testid="web-sync-status">{taskFor("web")?.status ?? "idle"}</output>
    </>
  );
}

function CancelSyncHarness() {
  const { startSync, cancelSync, task } = useConversationSync();

  return (
    <>
      <button
        onClick={() => {
          void startSync({ record_kind: "session" }).then((snapshot) => void cancelSync(snapshot.id));
        }}
        type="button"
      >
        Start and cancel sync
      </button>
      <output data-testid="cancel-sync-status">{task?.status ?? "idle"}</output>
    </>
  );
}
