// @vitest-environment jsdom

import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { SkillBackupProvider, useSkillBackup } from "./SkillBackupProvider";

const listenMock = vi.hoisted(() => vi.fn());
const getSkillBackupTaskMock = vi.hoisted(() => vi.fn());
const startSkillBackupTaskMock = vi.hoisted(() => vi.fn());

vi.mock("@tauri-apps/api/event", () => ({
  listen: listenMock,
}));

vi.mock("../../services/catalog", () => ({
  getSkillBackupTask: getSkillBackupTaskMock,
  startSkillBackupTask: startSkillBackupTaskMock,
}));

describe("SkillBackupProvider", () => {
  beforeEach(() => {
    listenMock.mockReset().mockResolvedValue(vi.fn());
    getSkillBackupTaskMock.mockReset().mockResolvedValue(null);
    startSkillBackupTaskMock.mockReset();
  });

  afterEach(() => {
    cleanup();
    vi.useRealTimers();
    vi.clearAllMocks();
  });

  it("keeps unrelated features interactive while backup progress updates", async () => {
    const runningTask = taskSnapshot("running", 0);
    startSkillBackupTaskMock.mockResolvedValue(runningTask);
    let backupListener: ((event: { payload: unknown }) => void) | undefined;
    listenMock.mockImplementation(
      async (_eventName: string, listener: (event: { payload: unknown }) => void) => {
        backupListener = listener;
        return vi.fn();
      },
    );

    render(
      <SkillBackupProvider>
        <ProviderHarness />
      </SkillBackupProvider>,
    );

    fireEvent.click(screen.getByRole("button", { name: "Start backup" }));
    await act(async () => {});
    expect(screen.getByRole<HTMLButtonElement>("button", { name: "Other feature" }).disabled).toBe(false);
    expect(screen.getByTestId("backup-status").textContent).toBe("running:0/2");

    await act(async () => {
      backupListener?.({ payload: taskSnapshot("completed", 2) });
    });
    expect(screen.getByTestId("backup-status").textContent).toBe("completed:2/2");
  });

  it("polls task status when a completion event is missed", async () => {
    vi.useFakeTimers();
    const runningTask = taskSnapshot("running", 0);
    startSkillBackupTaskMock.mockResolvedValue(runningTask);
    getSkillBackupTaskMock
      .mockResolvedValueOnce(null)
      .mockResolvedValueOnce(taskSnapshot("completed", 2));

    render(
      <SkillBackupProvider>
        <ProviderHarness />
      </SkillBackupProvider>,
    );

    fireEvent.click(screen.getByRole("button", { name: "Start backup" }));
    await act(async () => {});
    expect(screen.getByTestId("backup-status").textContent).toBe("running:0/2");

    await act(async () => {
      await vi.advanceTimersByTimeAsync(1000);
    });
    expect(screen.getByTestId("backup-status").textContent).toBe("completed:2/2");
  });
});

function ProviderHarness() {
  const { startBackup, task } = useSkillBackup();

  return (
    <>
      <button onClick={() => void startBackup(["skill-a", "skill-b"])} type="button">
        Start backup
      </button>
      <button type="button">Other feature</button>
      <output data-testid="backup-status">
        {task ? `${task.status}:${task.completed_count}/${task.total_count}` : "idle"}
      </output>
    </>
  );
}

function taskSnapshot(status: "running" | "completed", completedCount: number) {
  return {
    id: "skill-backup-1",
    status,
    asset_ids: ["skill-a", "skill-b"],
    total_count: 2,
    completed_count: completedCount,
    failed_count: 0,
    current_asset_id: status === "running" ? "skill-a" : null,
    started_at: "2026-06-18T00:00:00Z",
    finished_at: status === "completed" ? "2026-06-18T00:00:05Z" : null,
    assets: [],
    errors: [],
    error: null,
  } as const;
}
