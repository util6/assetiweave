// @vitest-environment jsdom

import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { SearchIndexProvider, useSearchIndex } from "./SearchIndexProvider";

const listenMock = vi.hoisted(() => vi.fn());
const statusMock = vi.hoisted(() => vi.fn());
const taskMock = vi.hoisted(() => vi.fn());
const rebuildMock = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/api/event", () => ({ listen: listenMock }));
vi.mock("../../services/conversations", () => ({
  getConversationSearchIndexStatus: statusMock,
  getConversationSearchIndexTask: taskMock,
  startConversationSearchIndexRebuild: rebuildMock,
}));

describe("SearchIndexProvider", () => {
  beforeEach(() => {
    listenMock.mockReset().mockResolvedValue(vi.fn());
    statusMock.mockReset().mockResolvedValue({ health: "missing" });
    taskMock.mockReset().mockResolvedValue(null);
    rebuildMock.mockReset();
  });

  afterEach(() => {
    cleanup();
    vi.useRealTimers();
    vi.clearAllMocks();
  });

  it("polls a running rebuild while unrelated controls remain enabled", async () => {
    vi.useFakeTimers();
    const running = {
      id: "index-1",
      status: "running",
      started_at: "2026-07-22T00:00:00Z",
      finished_at: null,
      result: null,
      error: null,
    } as const;
    rebuildMock.mockResolvedValue(running);
    taskMock
      .mockResolvedValueOnce(null)
      .mockResolvedValueOnce({ ...running, status: "completed", finished_at: "2026-07-22T00:00:01Z" });
    statusMock
      .mockResolvedValueOnce({ health: "missing" })
      .mockResolvedValueOnce({ health: "ready" });

    render(
      <SearchIndexProvider>
        <Harness />
      </SearchIndexProvider>,
    );
    fireEvent.click(screen.getByRole("button", { name: "Rebuild" }));
    await act(async () => {});
    expect(screen.getByTestId("task-status").textContent).toBe("running");
    expect((screen.getByRole("button", { name: "Other" }) as HTMLButtonElement).disabled).toBe(false);

    await act(async () => {
      await vi.advanceTimersByTimeAsync(1000);
    });
    expect(screen.getByTestId("task-status").textContent).toBe("completed");
    expect(screen.getByTestId("index-health").textContent).toBe("ready");
  });
});

function Harness() {
  const { rebuild, status, task } = useSearchIndex();
  return (
    <>
      <button onClick={() => void rebuild()} type="button">Rebuild</button>
      <button type="button">Other</button>
      <output data-testid="task-status">{task?.status ?? "idle"}</output>
      <output data-testid="index-health">{status?.health ?? "unknown"}</output>
    </>
  );
}
