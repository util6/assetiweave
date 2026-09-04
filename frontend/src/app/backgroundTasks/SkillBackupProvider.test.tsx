// @vitest-environment jsdom

import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";

import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { SkillBackupTaskSnapshot } from "../../services/catalog";
import { SkillBackupProvider, useSkillBackup } from "./SkillBackupProvider";

const subscribeSkillBackupTasksMock = vi.hoisted(() => vi.fn());
const getSkillBackupTaskMock = vi.hoisted(() => vi.fn());
const startSkillBackupTaskMock = vi.hoisted(() => vi.fn());

vi.mock("../../services/catalog", () => ({
  getSkillBackupTask: getSkillBackupTaskMock,
  startSkillBackupTask: startSkillBackupTaskMock,
  subscribeSkillBackupTasks: subscribeSkillBackupTasksMock,
}));

describe("SkillBackupProvider", () => {
  let queryClient: QueryClient;

  beforeEach(() => {
    queryClient = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    });
    subscribeSkillBackupTasksMock.mockReset().mockResolvedValue(vi.fn());
    getSkillBackupTaskMock.mockReset().mockResolvedValue(null);
    startSkillBackupTaskMock.mockReset();
  });

  afterEach(() => {
    cleanup();
    vi.useRealTimers();
    vi.clearAllMocks();
    queryClient.clear();
  });

  it("keeps unrelated features interactive while backup progress updates", async () => {
    const runningTask = taskSnapshot("running", 0);
    startSkillBackupTaskMock.mockResolvedValue(runningTask);
    let backupListener: ((snapshot: unknown) => void) | undefined;
    subscribeSkillBackupTasksMock.mockImplementation(
      async (listener: (snapshot: unknown) => void) => {
        backupListener = listener;
        return vi.fn();
      },
    );

    render(
      <QueryClientProvider client={queryClient}>
        <SkillBackupProvider>
          <ProviderHarness />
        </SkillBackupProvider>
      </QueryClientProvider>,
    );

    await act(async () => {});
    fireEvent.click(screen.getByRole("button", { name: "Start backup" }));
    await waitFor(() => {
      expect(screen.getByTestId("backup-status").textContent).toBe(
        "running:0/2",
      );
    });
    expect(
      screen.getByRole<HTMLButtonElement>("button", { name: "Other feature" })
        .disabled,
    ).toBe(false);

    await act(async () => {
      backupListener?.(taskSnapshot("completed", 2));
    });
    await waitFor(() => {
      expect(screen.getByTestId("backup-status").textContent).toBe(
        "completed:2/2",
      );
    });
  });

  it("polls task status when a completion event is missed", async () => {
    vi.useFakeTimers();
    const runningTask = taskSnapshot("running", 0);
    startSkillBackupTaskMock.mockResolvedValue(runningTask);
    getSkillBackupTaskMock
      .mockResolvedValueOnce(null)
      .mockResolvedValueOnce(taskSnapshot("completed", 2));

    render(
      <QueryClientProvider client={queryClient}>
        <SkillBackupProvider>
          <ProviderHarness />
        </SkillBackupProvider>
      </QueryClientProvider>,
    );

    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Start backup" }));
      await vi.advanceTimersByTimeAsync(0);
    });
    expect(screen.getByTestId("backup-status").textContent).toBe("running:0/2");

    await act(async () => {
      await vi.advanceTimersByTimeAsync(1000);
      await vi.advanceTimersByTimeAsync(1);
    });
    expect(screen.getByTestId("backup-status").textContent).toBe(
      "completed:2/2",
    );
  });
});

function ProviderHarness() {
  const { startBackup, task } = useSkillBackup();

  return (
    <>
      <button
        onClick={() => void startBackup(["skill-1", "skill-2"])}
        type="button"
      >
        Start backup
      </button>
      <button type="button">Other feature</button>
      <output data-testid="backup-status">
        {task
          ? `${task.status}:${task.completed_count}/${task.total_count}`
          : "idle"}
      </output>
    </>
  );
}

function taskSnapshot(
  status: "running" | "completed",
  current: number,
): SkillBackupTaskSnapshot {
  return {
    id: "backup-1",
    status,
    asset_ids: ["skill-1", "skill-2"],
    total_count: 2,
    completed_count: current,
    failed_count: 0,
    current_asset_id: current === 0 ? "skill-1" : "skill-2",
    started_at: "2026-03-31T00:00:00Z",
    finished_at: status === "completed" ? "2026-03-31T00:01:00Z" : null,
    assets: [],
    errors: [],
    error: null,
  };
}
