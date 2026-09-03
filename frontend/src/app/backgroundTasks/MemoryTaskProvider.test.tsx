// @vitest-environment jsdom

import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { MemoryTaskProvider, useMemoryTasks } from "./MemoryTaskProvider";

const listMock = vi.hoisted(() => vi.fn());
const cancelMock = vi.hoisted(() => vi.fn());
const retryMock = vi.hoisted(() => vi.fn());
const subscribeMock = vi.hoisted(() => vi.fn());
vi.mock("../../services/memory", () => ({
  listMemoryPublicTasks: listMock,
  cancelMemoryPublicTask: cancelMock,
  retryMemoryPublicTask: retryMock,
  subscribeMemoryTasks: subscribeMock,
}));

describe("MemoryTaskProvider", () => {
  beforeEach(() => {
    vi.useFakeTimers();
    listMock.mockReset().mockResolvedValue([]);
    cancelMock.mockReset();
    retryMock.mockReset();
    subscribeMock.mockReset().mockResolvedValue(vi.fn());
  });
  afterEach(() => {
    cleanup();
    vi.useRealTimers();
  });

  it("polls the public TaskRuntime without disabling unrelated controls", async () => {
    const running = task("running");
    listMock.mockResolvedValueOnce([running]).mockResolvedValueOnce([task("succeeded")]);
    render(<MemoryTaskProvider><Harness /></MemoryTaskProvider>);
    await act(async () => {});
    expect(screen.getByTestId("status").textContent).toBe("running");
    expect((screen.getByRole("button", { name: "Other" }) as HTMLButtonElement).disabled).toBe(false);
    await act(async () => {
      await vi.advanceTimersByTimeAsync(1000);
    });
    expect(screen.getByTestId("status").textContent).toBe("succeeded");
    expect(listMock).toHaveBeenCalledTimes(2);
    expect(listMock).toHaveBeenNthCalledWith(1, true);
    expect(listMock).toHaveBeenNthCalledWith(2, true);
  });

  it("refreshes immediately when the desktop task event arrives", async () => {
    const running = task("running");
    const completed = task("succeeded");
    let notify: (() => void) | undefined;
    subscribeMock.mockImplementation((listener: () => void) => {
      notify = listener;
      return Promise.resolve(vi.fn());
    });
    listMock.mockResolvedValueOnce([running]).mockResolvedValueOnce([completed]);

    render(<MemoryTaskProvider><Harness /></MemoryTaskProvider>);
    await act(async () => {});
    expect(screen.getByTestId("status").textContent).toBe("running");

    await act(async () => {
      notify?.();
    });
    expect(screen.getByTestId("status").textContent).toBe("succeeded");
  });

  it("exposes cancellation and retry through public task methods", async () => {
    const failed = task("failed");
    listMock.mockResolvedValue([failed]);
    cancelMock.mockResolvedValue({ ...failed, status: "cancelling" });
    retryMock.mockResolvedValue({ ...failed, status: "pending" });
    render(<MemoryTaskProvider><ActionHarness /></MemoryTaskProvider>);
    await act(async () => {});
    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    fireEvent.click(screen.getByRole("button", { name: "Retry" }));
    await act(async () => {});
    expect(cancelMock).toHaveBeenCalledWith(failed.id);
    expect(retryMock).toHaveBeenCalledWith(failed.id);
  });
});

function Harness() {
  const { task } = useMemoryTasks();
  return <><button type="button">Other</button><output data-testid="status">{task?.status ?? "idle"}</output></>;
}

function ActionHarness() {
  const { cancelTask, retryTask, task } = useMemoryTasks();
  return <>
    <button onClick={() => void cancelTask(task?.id ?? "")} type="button">Cancel</button>
    <button onClick={() => void retryTask(task?.id ?? "")} type="button">Retry</button>
  </>;
}

function task(status: "running" | "succeeded" | "failed" | "cancelling" | "pending") {
  return {
    id: "memory-task-1",
    status,
    kind: "memory",
    progress: { current: status === "running" ? 1 : 3, total: 3, note: null },
    started_at: "2026-09-01T00:00:00Z",
    finished_at: status === "succeeded" || status === "failed" ? "2026-09-01T00:00:05Z" : null,
    result: null,
    error: null,
    detail: { domain: "project_memory", job_id: "job-1" },
  } as const;
}
