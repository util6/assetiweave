// @vitest-environment jsdom

import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  ConversationDataMaintenanceProvider,
  useConversationDataMaintenance,
} from "./ConversationDataMaintenanceProvider";

const subscribeMock = vi.hoisted(() => vi.fn());
const listMock = vi.hoisted(() => vi.fn());
const auditMock = vi.hoisted(() => vi.fn());
const repairMock = vi.hoisted(() => vi.fn());
const cancelMock = vi.hoisted(() => vi.fn());

vi.mock("../../services/conversations", () => ({
  subscribeConversationDataMaintenanceTasks: subscribeMock,
  listConversationDataMaintenanceTasks: listMock,
  auditConversationData: auditMock,
  repairConversationData: repairMock,
  cancelConversationDataMaintenance: cancelMock,
}));

describe("ConversationDataMaintenanceProvider", () => {
  beforeEach(() => {
    subscribeMock.mockReset().mockResolvedValue(vi.fn());
    listMock.mockReset().mockResolvedValue([]);
    auditMock.mockReset();
    repairMock.mockReset();
    cancelMock.mockReset();
  });

  afterEach(() => {
    cleanup();
    vi.useRealTimers();
    vi.clearAllMocks();
  });

  it("merges audit events and keeps unrelated controls interactive", async () => {
    const runningTask = maintenanceTask("audit-1", "running");
    auditMock.mockResolvedValue(runningTask);
    let listener: ((snapshot: unknown) => void) | undefined;
    subscribeMock.mockImplementation(async (next: (snapshot: unknown) => void) => {
      listener = next;
      return vi.fn();
    });

    render(
      <ConversationDataMaintenanceProvider>
        <MaintenanceHarness />
      </ConversationDataMaintenanceProvider>,
    );

    fireEvent.click(screen.getByRole("button", { name: "Start audit" }));
    await act(async () => {});
    expect(screen.getByTestId("maintenance-status").textContent).toBe("running");
    expect((screen.getByRole("button", { name: "Other feature" }) as HTMLButtonElement).disabled).toBe(false);

    await act(async () => {
      listener?.(maintenanceTask("audit-1", "completed"));
    });
    expect(screen.getByTestId("maintenance-status").textContent).toBe("completed");
  });

  it("polls when an event is missed and exposes cancellation", async () => {
    vi.useFakeTimers();
    const runningTask = maintenanceTask("repair-1", "running");
    repairMock.mockResolvedValue(runningTask);
    listMock
      .mockResolvedValueOnce([])
      .mockResolvedValueOnce([maintenanceTask("repair-1", "completed")]);
    cancelMock.mockResolvedValue(maintenanceTask("repair-1", "cancelling"));

    render(
      <ConversationDataMaintenanceProvider>
        <MaintenanceHarness />
      </ConversationDataMaintenanceProvider>,
    );

    fireEvent.click(screen.getByRole("button", { name: "Start repair" }));
    await act(async () => {});
    expect(screen.getByTestId("maintenance-status").textContent).toBe("running");

    await act(async () => {
      await vi.advanceTimersByTimeAsync(1000);
    });
    expect(screen.getByTestId("maintenance-status").textContent).toBe("completed");

    fireEvent.click(screen.getByRole("button", { name: "Cancel maintenance" }));
    await act(async () => {});
    expect(cancelMock).toHaveBeenCalledWith("repair-1");
    expect(screen.getByTestId("maintenance-status").textContent).toBe("cancelling");
  });
});

function MaintenanceHarness() {
  const { audit, repair, cancel, task } = useConversationDataMaintenance();

  return (
    <>
      <button onClick={() => void audit({ record_kind: "session" })} type="button">Start audit</button>
      <button onClick={() => void repair({ dry_run: true })} type="button">Start repair</button>
      <button onClick={() => void (task && cancel(task.id))} type="button">Cancel maintenance</button>
      <button type="button">Other feature</button>
      <output data-testid="maintenance-status">{task?.status ?? "idle"}</output>
    </>
  );
}

function maintenanceTask(id: string, status: "running" | "completed" | "cancelling") {
  return {
    id,
    status,
    operation: id.startsWith("audit") ? "audit" : "repair",
    source_id: null,
    record_kind: null,
    dry_run: false,
    progress: { phase: status, completed_stage: status === "completed" ? 10 : 1, total_stage: 10, note: null },
    started_at: "2026-08-25T00:00:00Z",
    finished_at: status === "completed" ? "2026-08-25T00:00:05Z" : null,
    result: null,
    error: null,
  } as const;
}
