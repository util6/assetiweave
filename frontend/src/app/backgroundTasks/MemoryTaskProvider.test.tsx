// @vitest-environment jsdom

import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { MemoryTaskProvider, useMemoryTasks } from "./MemoryTaskProvider";

const listeners = vi.hoisted(() => new Map<string, (snapshot: unknown) => void>());
const subscribeMemoryTasksMock = vi.hoisted(() => vi.fn());
const subscribeConversationSyncTasksMock = vi.hoisted(() => vi.fn());
const listTasksMock = vi.hoisted(() => vi.fn());
const statusMock = vi.hoisted(() => vi.fn());
const startMock = vi.hoisted(() => vi.fn());
const cancelMock = vi.hoisted(() => vi.fn());

vi.mock("../../services/conversations", () => ({
  subscribeConversationSyncTasks: subscribeConversationSyncTasksMock,
}));
vi.mock("../../services/memory", () => ({
  cancelMemoryTask: cancelMock,
  getMemoryDreamStatus: statusMock,
  listMemoryTasks: listTasksMock,
  startMemoryTask: startMock,
  subscribeMemoryTasks: subscribeMemoryTasksMock,
}));

describe("MemoryTaskProvider", () => {
  beforeEach(() => {
    listeners.clear();
    subscribeMemoryTasksMock.mockReset().mockImplementation(async (listener: (snapshot: unknown) => void) => {
      listeners.set("memory-task-updated", listener);
      return vi.fn();
    });
    subscribeConversationSyncTasksMock.mockReset().mockImplementation(async (listener: (snapshot: unknown) => void) => {
      listeners.set("conversation-sync-task-updated", listener);
      return vi.fn();
    });
    listTasksMock.mockReset().mockResolvedValue([]);
    statusMock.mockReset().mockResolvedValue(null);
    startMock.mockReset();
    cancelMock.mockReset();
  });

  afterEach(() => {
    cleanup();
    vi.useRealTimers();
    vi.clearAllMocks();
  });

  it("recovers a completed task through polling while unrelated controls stay enabled", async () => {
    vi.useFakeTimers();
    const running = task("running");
    listTasksMock
      .mockResolvedValueOnce([running])
      .mockResolvedValueOnce([{ ...running, status: "completed", phase: "completed", finished_at: "2026-07-23T00:00:05Z" }]);

    render(<MemoryTaskProvider><Harness /></MemoryTaskProvider>);
    await act(async () => {});
    expect(screen.getByTestId("status").textContent).toBe("running");
    expect((screen.getByRole("button", { name: "Other" }) as HTMLButtonElement).disabled).toBe(false);

    await act(async () => {
      await vi.advanceTimersByTimeAsync(1000);
    });
    expect(screen.getByTestId("status").textContent).toBe("completed");
  });

  it("checks gates after a completed sync and deduplicates the automatic start", async () => {
    vi.useFakeTimers();
    const preview = {
      ready: true,
      scope: { app_id: null, source_id: null, project_path: null, session_id: null },
      scope_fingerprint: "scope-1",
      source_revision_end: 3,
      cursor_end: { session_sort_key: "cursor", question_offset: 1 },
    };
    statusMock.mockResolvedValue(preview);
    startMock.mockResolvedValue(task("running"));

    render(<MemoryTaskProvider><Harness /></MemoryTaskProvider>);
    await act(async () => {});
    await act(async () => {
      listeners.get("conversation-sync-task-updated")?.({ status: "completed" });
    });
    await act(async () => {
      listeners.get("conversation-sync-task-updated")?.({ status: "completed" });
    });

    expect(startMock).toHaveBeenCalledTimes(1);
    expect(startMock).toHaveBeenCalledWith(expect.objectContaining({
      kind: "auto_dream",
      trigger: "automatic",
    }));
  });

  it("starts deep Recall as a background task with the complete local scope", async () => {
    startMock.mockResolvedValue({ ...task("running"), kind: "deep_recall", phase: "phase1" });
    render(<MemoryTaskProvider><Harness /></MemoryTaskProvider>);
    await act(async () => {});

    fireEvent.click(screen.getByRole("button", { name: "Recall" }));
    await act(async () => {});

    expect(startMock).toHaveBeenCalledWith(expect.objectContaining({
      kind: "deep_recall",
      synthesize: true,
      recall: expect.objectContaining({ mode: "exact", query: "why" }),
    }));
  });
});

function Harness() {
  const { task, startDream, startRecall } = useMemoryTasks();
  return (
    <>
      <button onClick={() => void startDream()} type="button">Start</button>
      <button onClick={() => void startRecall({ mode: "exact", query: "why", synthesize: true })} type="button">Recall</button>
      <button type="button">Other</button>
      <output data-testid="status">{task?.status ?? "idle"}</output>
    </>
  );
}

function task(status: "running" | "completed") {
  return {
    id: "memory-task-1",
    status,
    kind: "auto_dream",
    scope: { app_id: null, source_id: null, project_path: null, session_id: null },
    scope_fingerprint: "scope-1",
    trigger: "manual",
    dry_run: false,
    phase: status === "running" ? "dreaming" : "completed",
    processed_count: status === "running" ? 1 : 3,
    total_count: 3,
    run_id: "run-1",
    cancel_requested: false,
    started_at: "2026-07-23T00:00:00Z",
    finished_at: status === "completed" ? "2026-07-23T00:00:05Z" : null,
    result: null,
    error: null,
  } as const;
}
