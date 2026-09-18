/* @vitest-environment jsdom */
import { beforeEach, describe, expect, it, vi } from "vitest";

const invokeMock = vi.hoisted(() => vi.fn());
const listenMock = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/api/core", () => ({
  invoke: invokeMock,
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: listenMock,
}));

import {
  listPublicTasks,
  getPublicTask,
  cancelPublicTask,
  retryPublicTask,
  clearTerminalTasks,
  subscribeTaskUpdated,
} from "./taskCenterService";
import type { TaskView } from "../types/taskCenter";

describe("taskCenterService", () => {
  beforeEach(() => {
    invokeMock.mockReset();
    listenMock.mockReset();
    // mock window.__TAURI_INTERNALS__ to simulate desktop
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    (window as any).__TAURI_INTERNALS__ = {};
  });

  it("lists public tasks with parameters", async () => {
    const mockTasks: TaskView[] = [
      {
        id: "task-1",
        kind: "conversation_sync",
        title: "会话同步",
        state: "running",
        stages: [],
        metrics: [],
        failures: [],
        capabilities: { cancellable: true, retryable: false, clearable: false },
        revision: 1,
      },
    ];
    invokeMock.mockResolvedValueOnce(mockTasks);

    const result = await listPublicTasks({ active_only: true });
    expect(invokeMock).toHaveBeenCalledWith("list_public_tasks", {
      params: { active_only: true },
    });
    expect(result).toEqual(mockTasks);
  });

  it("gets single task detail", async () => {
    const mockTask: TaskView = {
      id: "task-1",
      kind: "memory",
      title: "记忆生成",
      state: "succeeded",
      outcome: "success",
      stages: [],
      metrics: [],
      failures: [],
      capabilities: { cancellable: false, retryable: false, clearable: true },
      revision: 2,
    };
    invokeMock.mockResolvedValueOnce(mockTask);

    const result = await getPublicTask("task-1");
    expect(invokeMock).toHaveBeenCalledWith("get_public_task", {
      params: { task_id: "task-1" },
    });
    expect(result).toEqual(mockTask);
  });

  it("cancels a running task", async () => {
    const mockTask: TaskView = {
      id: "task-1",
      kind: "memory",
      title: "记忆生成",
      state: "cancelling",
      stages: [],
      metrics: [],
      failures: [],
      capabilities: { cancellable: false, retryable: false, clearable: false },
      revision: 3,
    };
    invokeMock.mockResolvedValueOnce(mockTask);

    const result = await cancelPublicTask("task-1");
    expect(invokeMock).toHaveBeenCalledWith("cancel_public_task", {
      params: { task_id: "task-1" },
    });
    expect(result.state).toBe("cancelling");
  });

  it("retries a failed task", async () => {
    const mockTask: TaskView = {
      id: "task-1-retry",
      kind: "memory",
      title: "记忆生成",
      state: "pending",
      stages: [],
      metrics: [],
      failures: [],
      capabilities: { cancellable: true, retryable: false, clearable: false },
      revision: 1,
    };
    invokeMock.mockResolvedValueOnce(mockTask);

    const result = await retryPublicTask("task-1");
    expect(invokeMock).toHaveBeenCalledWith("retry_public_task", {
      params: { task_id: "task-1" },
    });
    expect(result.id).toBe("task-1-retry");
  });

  it("clears terminal tasks", async () => {
    invokeMock.mockResolvedValueOnce(3);

    const clearedCount = await clearTerminalTasks({ tenant_id: "tenant-a" });
    expect(invokeMock).toHaveBeenCalledWith("clear_terminal_tasks", {
      params: { tenant_id: "tenant-a" },
    });
    expect(clearedCount).toBe(3);
  });

  it("subscribes to task-updated events", async () => {
    const unsubscribe = vi.fn();
    listenMock.mockResolvedValueOnce(unsubscribe);

    const callback = vi.fn();
    const unlisten = await subscribeTaskUpdated(callback);

    expect(listenMock).toHaveBeenCalledWith(
      "task-updated",
      expect.any(Function),
    );
    const eventHandler = listenMock.mock.calls[0][1];
    const taskPayload: TaskView = {
      id: "task-event-1",
      kind: "scan",
      title: "数据源扫描",
      state: "running",
      stages: [],
      metrics: [],
      failures: [],
      capabilities: { cancellable: true, retryable: false, clearable: false },
      revision: 1,
    };
    eventHandler({ payload: taskPayload });
    expect(callback).toHaveBeenCalledWith(taskPayload);

    expect(unlisten).toBe(unsubscribe);
  });
});
