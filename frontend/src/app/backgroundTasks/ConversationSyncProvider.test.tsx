// @vitest-environment jsdom

import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { ConversationSyncProvider, useConversationSync } from "./ConversationSyncProvider";

const listenMock = vi.hoisted(() => vi.fn());
const getConversationSyncTaskMock = vi.hoisted(() => vi.fn());
const syncConversationsMock = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/api/event", () => ({
  listen: listenMock,
}));

vi.mock("../../services/conversations", () => ({
  getConversationSyncTask: getConversationSyncTaskMock,
  syncConversations: syncConversationsMock,
}));

describe("ConversationSyncProvider", () => {
  beforeEach(() => {
    listenMock.mockReset().mockResolvedValue(vi.fn());
    getConversationSyncTaskMock.mockReset().mockResolvedValue(null);
    syncConversationsMock.mockReset();
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
    let syncListener: ((event: { payload: unknown }) => void) | undefined;
    listenMock.mockImplementation(
      async (_eventName: string, listener: (event: { payload: unknown }) => void) => {
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
        payload: {
          ...runningTask,
          status: "completed",
          finished_at: "2026-06-15T00:00:05Z",
          result: { results: [] },
        },
      });
    });
    expect(screen.getByTestId("sync-status").textContent).toBe("completed");
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
    getConversationSyncTaskMock
      .mockResolvedValueOnce(null)
      .mockResolvedValueOnce({
        ...runningTask,
        status: "completed",
        finished_at: "2026-06-15T00:00:05Z",
        result: { results: [] },
      });

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
