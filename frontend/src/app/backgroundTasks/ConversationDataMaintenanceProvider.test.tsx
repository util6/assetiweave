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
  let queryClient: QueryClient;

  beforeEach(() => {
    queryClient = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    });
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
    queryClient.clear();
  });

  function renderWithClient(ui: React.ReactElement) {
    return render(
      <QueryClientProvider client={queryClient}>
        <ConversationDataMaintenanceProvider>
          {ui}
        </ConversationDataMaintenanceProvider>
      </QueryClientProvider>,
    );
  }

  it("merges audit events and keeps unrelated controls interactive", async () => {
    const runningTask = maintenanceTask("audit-1", "running");
    auditMock.mockResolvedValue(runningTask);
    let listener: ((snapshot: unknown) => void) | undefined;
    subscribeMock.mockImplementation(
      async (next: (snapshot: unknown) => void) => {
        listener = next;
        return vi.fn();
      },
    );

    renderWithClient(<MaintenanceHarness />);

    fireEvent.click(screen.getByRole("button", { name: "Start audit" }));
    await waitFor(() => {
      expect(screen.getByTestId("maintenance-status").textContent).toBe(
        "running",
      );
    });
    expect(
      (
        screen.getByRole("button", {
          name: "Other feature",
        }) as HTMLButtonElement
      ).disabled,
    ).toBe(false);

    await act(async () => {
      listener?.(maintenanceTask("audit-1", "completed"));
    });
    await waitFor(() => {
      expect(screen.getByTestId("maintenance-status").textContent).toBe(
        "completed",
      );
    });
  });

  it("polls when an event is missed and exposes cancellation", async () => {
    vi.useFakeTimers();
    const runningTask = maintenanceTask("repair-1", "running");
    repairMock.mockResolvedValue(runningTask);
    listMock
      .mockResolvedValueOnce([])
      .mockResolvedValueOnce([maintenanceTask("repair-1", "completed")]);
    cancelMock.mockResolvedValue(maintenanceTask("repair-1", "cancelling"));

    renderWithClient(<MaintenanceHarness />);

    await act(async () => {
      fireEvent.click(screen.getByRole("button", { name: "Start repair" }));
      await vi.advanceTimersByTimeAsync(0);
    });
    expect(screen.getByTestId("maintenance-status").textContent).toBe(
      "running",
    );

    await act(async () => {
      await vi.advanceTimersByTimeAsync(1000);
      await vi.advanceTimersByTimeAsync(1);
    });
    expect(screen.getByTestId("maintenance-status").textContent).toBe(
      "completed",
    );

    await act(async () => {
      fireEvent.click(
        screen.getByRole("button", { name: "Cancel maintenance" }),
      );
      await vi.advanceTimersByTimeAsync(0);
    });
    expect(cancelMock).toHaveBeenCalledWith("repair-1");
    expect(screen.getByTestId("maintenance-status").textContent).toBe(
      "cancelling",
    );
  });
});

function MaintenanceHarness() {
  const { audit, cancel, repair, task } = useConversationDataMaintenance();

  return (
    <>
      <button onClick={() => void audit()} type="button">
        Start audit
      </button>
      <button onClick={() => void repair()} type="button">
        Start repair
      </button>
      <button onClick={() => void cancel(task?.id ?? "")} type="button">
        Cancel maintenance
      </button>
      <button type="button">Other feature</button>
      <output data-testid="maintenance-status">{task?.status ?? "idle"}</output>
    </>
  );
}

function maintenanceTask(
  id: string,
  status: "running" | "completed" | "cancelling",
) {
  return {
    id,
    action: "audit",
    status,
    total: 10,
    current: status === "completed" ? 10 : 3,
    issues_found: 0,
    issues_repaired: 0,
    dry_run: false,
    started_at: "2026-06-15T00:00:00Z",
    finished_at: status === "completed" ? "2026-06-15T00:00:10Z" : null,
    error: null,
  } as const;
}
