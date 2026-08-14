// @vitest-environment jsdom

import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { AiExecutionTaskIndicator } from "./AiExecutionTaskIndicator";

const cancelTaskMock = vi.hoisted(() => vi.fn());
const tasks = vi.hoisted(() => ({ value: [] as Array<Record<string, unknown>> }));

vi.mock("./AiExecutionTaskProvider", () => ({
  isActiveAiExecutionTask: (task: { state: string }) => (
    task.state === "queued" || task.state === "running"
  ),
  useAiExecutionTasks: () => ({
    cancelTask: cancelTaskMock,
    tasks: tasks.value,
  }),
}));

vi.mock("../../i18n/I18nProvider", () => ({
  useI18n: () => ({
    t: (key: string, params?: Record<string, unknown>) => (
      params?.count == null ? key : `${key}:${params.count}`
    ),
  }),
}));

describe("AiExecutionTaskIndicator", () => {
  beforeEach(() => {
    cancelTaskMock.mockReset().mockResolvedValue(undefined);
    tasks.value = [];
  });

  afterEach(() => cleanup());

  it("stays hidden without active Agent tasks", () => {
    render(<AiExecutionTaskIndicator />);
    expect(screen.queryByRole("status")).toBeNull();
  });

  it("shows active count, latest phase, and cancels the latest task", async () => {
    tasks.value = [
      task("ai-task-1", "queued", "queued", "2026-08-13T00:00:01Z"),
      task("ai-task-2", "running", "prompting", "2026-08-13T00:00:03Z"),
      task("ai-task-done", "succeeded", "cleaning_up", "2026-08-13T00:00:04Z"),
    ];

    render(<AiExecutionTaskIndicator />);

    expect(screen.getByText("ai.execution.global.title:2")).toBeTruthy();
    expect(screen.getByText("ai.execution.phase.prompting")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "ai.execution.cancel" }));
    await waitFor(() => expect(cancelTaskMock).toHaveBeenCalledWith("ai-task-2"));
  });

  it("reports a cancellation request failure without an unhandled rejection", async () => {
    cancelTaskMock.mockRejectedValueOnce(new Error("cancel failed"));
    tasks.value = [
      task("ai-task-1", "running", "prompting", "2026-08-13T00:00:03Z"),
    ];

    render(<AiExecutionTaskIndicator />);
    fireEvent.click(screen.getByRole("button", { name: "ai.execution.cancel" }));

    expect((await screen.findByRole("alert")).textContent).toBe("ai.execution.cancelFailed");
  });
});

function task(
  id: string,
  state: string,
  phase: string,
  updatedAt: string,
) {
  return {
    id,
    purpose: "translation",
    agent_id: "opencode",
    state,
    phase,
    created_at: "2026-08-13T00:00:00Z",
    updated_at: updatedAt,
    finished_at: state === "succeeded" ? updatedAt : null,
    result: null,
    error: null,
  };
}
