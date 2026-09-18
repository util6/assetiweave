// @vitest-environment jsdom

import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
} from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { TaskCenterProvider, useTaskCenter } from "./TaskCenterProvider";
import type { TaskView } from "../../types/taskCenter";

const listMock = vi.hoisted(() => vi.fn());
const cancelMock = vi.hoisted(() => vi.fn());
const retryMock = vi.hoisted(() => vi.fn());
const clearMock = vi.hoisted(() => vi.fn());
const subscribeMock = vi.hoisted(() => vi.fn());

vi.mock("../../services/taskCenterService", () => ({
  listPublicTasks: listMock,
  cancelPublicTask: cancelMock,
  retryPublicTask: retryMock,
  clearTerminalTasks: clearMock,
  subscribeTaskUpdated: subscribeMock,
}));

const mockSettings = vi.hoisted(() => ({
  showTaskNotifications: true,
}));

vi.mock("../../store/settings/useAppSettings", () => ({
  useAppSettings: () => ({
    settings: mockSettings,
  }),
}));

describe("TaskCenterProvider", () => {
  let queryClient: QueryClient;

  beforeEach(() => {
    queryClient = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    });
    vi.useFakeTimers();
    listMock.mockReset().mockResolvedValue([]);
    cancelMock.mockReset();
    retryMock.mockReset();
    clearMock.mockReset();
    subscribeMock.mockReset().mockResolvedValue(vi.fn());
    mockSettings.showTaskNotifications = true;
  });

  afterEach(() => {
    cleanup();
    vi.useRealTimers();
    queryClient.clear();
  });

  it("provides tasks list and distinguishes active and terminal tasks", async () => {
    const runningTask = createTask("task-1", "running");
    const succeededTask = createTask("task-2", "succeeded");
    listMock.mockResolvedValueOnce([runningTask, succeededTask]);

    render(
      <QueryClientProvider client={queryClient}>
        <TaskCenterProvider>
          <Harness />
        </TaskCenterProvider>
      </QueryClientProvider>,
    );

    await act(async () => {
      await vi.advanceTimersByTimeAsync(0);
    });

    expect(screen.getByTestId("active-count").textContent).toBe("1");
    expect(screen.getByTestId("total-count").textContent).toBe("2");
    expect(screen.getByTestId("selected-task-id").textContent).toBe("task-1");
  });

  it("handles cancellation, retry and clearing terminal tasks", async () => {
    const failedTask = createTask("task-failed", "failed");
    listMock.mockResolvedValueOnce([failedTask]);
    cancelMock.mockResolvedValueOnce({ ...failedTask, state: "cancelling" });
    retryMock.mockResolvedValueOnce({
      ...failedTask,
      id: "task-failed-retry",
      state: "pending",
    });
    clearMock.mockResolvedValueOnce(1);

    render(
      <QueryClientProvider client={queryClient}>
        <TaskCenterProvider>
          <ActionHarness />
        </TaskCenterProvider>
      </QueryClientProvider>,
    );

    await act(async () => {
      await vi.advanceTimersByTimeAsync(0);
    });

    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    await act(async () => {
      await vi.advanceTimersByTimeAsync(0);
    });
    expect(cancelMock).toHaveBeenCalledWith("task-failed");

    fireEvent.click(screen.getByRole("button", { name: "Retry" }));
    await act(async () => {
      await vi.advanceTimersByTimeAsync(0);
    });
    expect(retryMock).toHaveBeenCalledWith("task-failed");

    fireEvent.click(screen.getByRole("button", { name: "Clear" }));
    await act(async () => {
      await vi.advanceTimersByTimeAsync(0);
    });
    expect(clearMock).toHaveBeenCalled();
  });

  it("enqueues notification when a running task completes", async () => {
    let notifyCallback: ((task: TaskView) => void) | undefined;
    subscribeMock.mockImplementation((cb: (task: TaskView) => void) => {
      notifyCallback = cb;
      return Promise.resolve(vi.fn());
    });

    const running = createTask("task-notify", "running");
    listMock.mockResolvedValueOnce([running]);

    render(
      <QueryClientProvider client={queryClient}>
        <TaskCenterProvider>
          <NotificationHarness />
        </TaskCenterProvider>
      </QueryClientProvider>,
    );

    await act(async () => {
      await vi.advanceTimersByTimeAsync(0);
    });

    expect(screen.getByTestId("notification").textContent).toBe("none");

    // Task transitions to succeeded
    const succeeded = {
      ...running,
      state: "succeeded" as const,
      outcome: "success" as const,
    };
    await act(async () => {
      notifyCallback?.(succeeded);
      await vi.advanceTimersByTimeAsync(0);
    });

    expect(screen.getByTestId("notification").textContent).toContain(
      "task-notify",
    );
  });
});

function Harness() {
  const { tasks, activeCount, selectedTask } = useTaskCenter();
  return (
    <div>
      <span data-testid="active-count">{activeCount}</span>
      <span data-testid="total-count">{tasks.length}</span>
      <span data-testid="selected-task-id">{selectedTask?.id ?? "none"}</span>
    </div>
  );
}

function ActionHarness() {
  const { cancelTask, retryTask, clearTerminal, tasks } = useTaskCenter();
  const first = tasks[0];
  return (
    <div>
      <button onClick={() => void cancelTask(first?.id ?? "")} type="button">
        Cancel
      </button>
      <button onClick={() => void retryTask(first?.id ?? "")} type="button">
        Retry
      </button>
      <button onClick={() => void clearTerminal()} type="button">
        Clear
      </button>
    </div>
  );
}

function NotificationHarness() {
  const { currentNotification, dismissNotification } = useTaskCenter();
  return (
    <div>
      <span data-testid="notification">
        {currentNotification ? currentNotification.task.id : "none"}
      </span>
      {currentNotification ? (
        <button
          onClick={() => dismissNotification(currentNotification.id)}
          type="button"
        >
          Dismiss
        </button>
      ) : null}
    </div>
  );
}

function createTask(
  id: string,
  state: "running" | "succeeded" | "failed" | "pending",
): TaskView {
  return {
    id,
    kind: "conversation_sync",
    title: `Task ${id}`,
    state,
    outcome:
      state === "succeeded"
        ? "success"
        : state === "failed"
          ? "failure"
          : undefined,
    started_at: "2026-09-10T12:00:00Z",
    finished_at:
      state === "succeeded" || state === "failed"
        ? "2026-09-10T12:01:00Z"
        : undefined,
    stages: [],
    metrics: [],
    failures: [],
    capabilities: {
      cancellable: state === "running",
      retryable: state === "failed",
      clearable: state === "succeeded" || state === "failed",
    },
    revision: 1,
  };
}
