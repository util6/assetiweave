// @vitest-environment jsdom

import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { TeamTaskProvider, useTeamTasks } from "./TeamTaskProvider";
import type { TeamRuntimeTaskSnapshot } from "../../types/team";

const subscribeMock = vi.hoisted(() => vi.fn());
const listMock = vi.hoisted(() => vi.fn());

vi.mock("../../services/teamWorkflow", () => ({
  listTeamRunTasks: listMock,
  subscribeTeamRunTasks: subscribeMock,
}));

describe("TeamTaskProvider", () => {
  let queryClient: QueryClient;

  beforeEach(() => {
    queryClient = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    });
    subscribeMock.mockReset().mockResolvedValue(vi.fn());
    listMock.mockReset().mockResolvedValue([]);
  });

  afterEach(() => {
    cleanup();
    vi.useRealTimers();
    vi.clearAllMocks();
    queryClient.clear();
  });

  function renderWithClient(ui: React.ReactElement) {
    return render(
      <QueryClientProvider client={queryClient}>
        <TeamTaskProvider>{ui}</TeamTaskProvider>
      </QueryClientProvider>,
    );
  }

  it("loads running team run tasks and updates via event bridge", async () => {
    const runningTask = mockTeamTask("task-1", "Running");
    listMock.mockResolvedValue([runningTask]);
    let listener: ((snapshot: TeamRuntimeTaskSnapshot) => void) | undefined;
    subscribeMock.mockImplementation(
      async (next: (snapshot: TeamRuntimeTaskSnapshot) => void) => {
        listener = next;
        return vi.fn();
      },
    );

    renderWithClient(<TeamTaskHarness />);

    await waitFor(() => {
      expect(screen.getByTestId("task-count").textContent).toBe("1");
      expect(screen.getByTestId("task-state").textContent).toBe("Running");
    });

    await act(async () => {
      listener?.({
        ...runningTask,
        state: "Succeeded",
      });
    });

    await waitFor(() => {
      expect(screen.getByTestId("task-state").textContent).toBe("Succeeded");
    });
  });

  it("polls running team tasks when events are delayed", async () => {
    vi.useFakeTimers();
    const runningTask = mockTeamTask("task-2", "Running");
    listMock
      .mockResolvedValueOnce([runningTask])
      .mockResolvedValueOnce([{ ...runningTask, state: "Succeeded" }]);

    renderWithClient(<TeamTaskHarness />);

    await act(async () => {
      await vi.advanceTimersByTimeAsync(0);
    });
    expect(screen.getByTestId("task-state").textContent).toBe("Running");

    await act(async () => {
      await vi.advanceTimersByTimeAsync(1000);
      await vi.advanceTimersByTimeAsync(1);
    });

    expect(screen.getByTestId("task-state").textContent).toBe("Succeeded");
  });
});

function TeamTaskHarness() {
  const { getTask, tasks } = useTeamTasks();
  const first = tasks[0];
  const byId = first ? getTask(first.task_id) : undefined;

  return (
    <>
      <output data-testid="task-count">{tasks.length}</output>
      <output data-testid="task-state">{byId?.state ?? "none"}</output>
    </>
  );
}

function mockTeamTask(
  taskId: string,
  state: TeamRuntimeTaskSnapshot["state"],
): TeamRuntimeTaskSnapshot {
  return {
    task_id: taskId,
    kind: "TeamRun",
    dedup_key: `team-1:${taskId}`,
    state,
    progress: null,
    started_at: "2026-09-01T00:00:00Z",
    finished_at: state === "Succeeded" ? "2026-09-01T00:00:10Z" : null,
    error: null,
    detail: null,
    result: null,
  };
}
