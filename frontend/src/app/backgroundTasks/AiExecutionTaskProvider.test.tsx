// @vitest-environment jsdom

import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  AiExecutionTaskProvider,
  mergeAiExecutionTaskSnapshots,
  useAiExecutionTasks,
} from "./AiExecutionTaskProvider";
import type {
  AiExecutionTaskSnapshot,
  ConversationCardTranslationRequest,
} from "../../services/cardTranslation";

const listeners = vi.hoisted(() => new Map<string, (snapshot: unknown) => void>());
const subscribeTasksMock = vi.hoisted(() => vi.fn());
const listTasksMock = vi.hoisted(() => vi.fn());
const startTaskMock = vi.hoisted(() => vi.fn());
const cancelTaskMock = vi.hoisted(() => vi.fn());

vi.mock("../../services/cardTranslation", () => ({
  cancelAiExecutionTask: cancelTaskMock,
  listAiExecutionTasks: listTasksMock,
  startConversationCardTranslation: startTaskMock,
  subscribeAiExecutionTasks: subscribeTasksMock,
}));

describe("AiExecutionTaskProvider", () => {
  beforeEach(() => {
    listeners.clear();
    subscribeTasksMock.mockReset().mockImplementation(async (listener: (snapshot: unknown) => void) => {
      listeners.set("ai-execution://task-updated", listener);
      return vi.fn();
    });
    listTasksMock.mockReset().mockResolvedValue([]);
    startTaskMock.mockReset();
    cancelTaskMock.mockReset();
  });

  afterEach(() => {
    cleanup();
    vi.useRealTimers();
    vi.clearAllMocks();
  });

  it("merges full snapshots by id and ignores an older event", async () => {
    const running = taskSnapshot("running", "prompting", "2026-08-13T00:00:02Z");
    listTasksMock.mockResolvedValue([running]);

    render(<AiExecutionTaskProvider><Harness /></AiExecutionTaskProvider>);
    await act(async () => {});
    expect(screen.getByTestId("state").textContent).toBe("running:prompting");

    await act(async () => {
      listeners.get("ai-execution://task-updated")?.(taskSnapshot("succeeded", "cleaning_up", "2026-08-13T00:00:03Z"));
      listeners.get("ai-execution://task-updated")?.(taskSnapshot("running", "initializing", "2026-08-13T00:00:01Z"));
    });

    expect(screen.getByTestId("state").textContent).toBe("succeeded:cleaning_up");
    expect(screen.getByTestId("count").textContent).toBe("1");
  });

  it("recovers a missed terminal event through polling", async () => {
    vi.useFakeTimers();
    const running = taskSnapshot("running", "prompting", "2026-08-13T00:00:02Z");
    listTasksMock
      .mockResolvedValueOnce([running])
      .mockResolvedValueOnce([
        taskSnapshot("succeeded", "cleaning_up", "2026-08-13T00:00:03Z"),
      ]);

    render(<AiExecutionTaskProvider><Harness /></AiExecutionTaskProvider>);
    await act(async () => {});
    expect(screen.getByTestId("state").textContent).toBe("running:prompting");

    await act(async () => {
      await vi.advanceTimersByTimeAsync(1000);
    });

    expect(screen.getByTestId("state").textContent).toBe("succeeded:cleaning_up");
  });

  it("starts and cancels through typed context methods without disabling unrelated controls", async () => {
    const queued = taskSnapshot("queued", "queued", "2026-08-13T00:00:01Z");
    startTaskMock.mockResolvedValue(queued);
    cancelTaskMock.mockResolvedValue({
      ...queued,
      state: "running",
      phase: "cancelling",
      updated_at: "2026-08-13T00:00:02Z",
    });

    render(<AiExecutionTaskProvider><Harness /></AiExecutionTaskProvider>);
    await act(async () => {});
    fireEvent.click(screen.getByRole("button", { name: "Start" }));
    await act(async () => {});
    expect(screen.getByTestId("state").textContent).toBe("queued:queued");
    expect((screen.getByRole("button", { name: "Other" }) as HTMLButtonElement).disabled).toBe(false);

    fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
    await act(async () => {});
    expect(cancelTaskMock).toHaveBeenCalledWith("ai-task-1");
    expect(screen.getByTestId("state").textContent).toBe("running:cancelling");
  });

  it("cleans up its listener and polling timer on unmount", async () => {
    vi.useFakeTimers();
    const unlisten = vi.fn();
    subscribeTasksMock.mockResolvedValue(unlisten);
    listTasksMock.mockResolvedValue([
      taskSnapshot("running", "prompting", "2026-08-13T00:00:02Z"),
    ]);

    const view = render(<AiExecutionTaskProvider><Harness /></AiExecutionTaskProvider>);
    await act(async () => {});
    expect(vi.getTimerCount()).toBe(1);

    view.unmount();
    expect(unlisten).toHaveBeenCalledTimes(1);
    expect(vi.getTimerCount()).toBe(0);
  });

  it("bounds retained terminal snapshots while preserving every active task", () => {
    const active = Array.from({ length: 3 }, (_, index) => ({
      ...taskSnapshot("running", "prompting", `2026-08-13T00:00:${index.toString().padStart(2, "0")}Z`),
      id: `active-${index}`,
    }));
    const terminal = Array.from({ length: 101 }, (_, index) => ({
      ...taskSnapshot("succeeded", "cleaning_up", `2026-08-13T00:${index.toString().padStart(2, "0")}:00Z`),
      id: `terminal-${index}`,
    }));

    const merged = mergeAiExecutionTaskSnapshots([], [...active, ...terminal]);

    expect(merged.filter((task) => task.state === "running")).toHaveLength(3);
    expect(merged.filter((task) => task.state === "succeeded")).toHaveLength(100);
    expect(merged.some((task) => task.id === "terminal-0")).toBe(false);
  });
});

const translationRequest: ConversationCardTranslationRequest = {
  agentId: "opencode",
  model: "",
  provider: "cli",
  targetLanguage: "zh-CN",
  text: "Translate me",
};

function Harness() {
  const { cancelTask, getTask, startTranslation, tasks } = useAiExecutionTasks();
  const task = getTask("ai-task-1") ?? tasks[0];
  return (
    <>
      <button onClick={() => void startTranslation(translationRequest)} type="button">Start</button>
      <button onClick={() => task && void cancelTask(task.id)} type="button">Cancel</button>
      <button type="button">Other</button>
      <output data-testid="state">
        {task ? `${task.state}:${task.phase}` : "idle"}
      </output>
      <output data-testid="count">{tasks.length}</output>
    </>
  );
}

function taskSnapshot(
  state: AiExecutionTaskSnapshot["state"],
  phase: AiExecutionTaskSnapshot["phase"],
  updatedAt: string,
): AiExecutionTaskSnapshot {
  return {
    id: "ai-task-1",
    purpose: "translation",
    agent_id: "opencode",
    state,
    phase,
    created_at: "2026-08-13T00:00:00Z",
    updated_at: updatedAt,
    finished_at: ["succeeded", "failed", "cancelled"].includes(state) ? updatedAt : null,
    result: state === "succeeded" ? { text: "translated" } : null,
    error: null,
    cleanup: null,
  };
}
