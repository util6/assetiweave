import { describe, expect, it } from "vitest";
import {
  checkEventTenantMatch,
  isTerminalStatus,
  mergeTaskSnapshot,
} from "./taskMergeUtils";
import { mergeConversationTaskSnapshot } from "./ConversationSyncProvider";
import type { TaskView } from "../../types/taskCenter";
import type { ConversationSyncTaskSnapshot } from "../../services/conversations";

describe("taskMergeUtils", () => {
  it("isTerminalStatus recognizes terminal statuses case-insensitively", () => {
    expect(isTerminalStatus("completed")).toBe(true);
    expect(isTerminalStatus("Completed")).toBe(true);
    expect(isTerminalStatus("failed")).toBe(true);
    expect(isTerminalStatus("Cancelled")).toBe(true);
    expect(isTerminalStatus("canceled")).toBe(true);
    expect(isTerminalStatus("success")).toBe(true);
    expect(isTerminalStatus("succeeded")).toBe(true);
    expect(isTerminalStatus("Succeeded")).toBe(true);

    expect(isTerminalStatus("running")).toBe(false);
    expect(isTerminalStatus("pending")).toBe(false);
    expect(isTerminalStatus("cancelling")).toBe(false);
    expect(isTerminalStatus(null)).toBe(false);
    expect(isTerminalStatus(undefined)).toBe(false);
  });

  it("mergeTaskSnapshot protects terminal status from out-of-order running polling", () => {
    const completedTask: TaskView = {
      id: "task-1",
      kind: "scan",
      state: "succeeded",
      title: "Scan sources",
      stages: [],
      metrics: [],
      failures: [],
      capabilities: { cancellable: false, retryable: false, clearable: true },
      revision: 2,
      started_at: "2026-09-04T00:00:00Z",
      finished_at: "2026-09-04T00:02:00Z",
    };

    const stalePollingRunningTask: TaskView = {
      id: "task-1",
      kind: "scan",
      state: "running",
      title: "Scan sources",
      stages: [],
      metrics: [],
      failures: [],
      capabilities: { cancellable: true, retryable: false, clearable: false },
      revision: 1,
      started_at: "2026-09-04T00:00:00Z",
      finished_at: null,
    };

    const merged = mergeTaskSnapshot(stalePollingRunningTask, completedTask);
    expect(merged.state).toBe("succeeded");
    expect(merged.finished_at).toBe("2026-09-04T00:02:00Z");
  });

  it("checkEventTenantMatch validates tenant matching or returns unknown when absent", () => {
    expect(checkEventTenantMatch({ tenant_id: "tenant-a" }, "tenant-a")).toBe(
      true,
    );
    expect(checkEventTenantMatch({ tenant_id: "tenant-b" }, "tenant-a")).toBe(
      false,
    );
    expect(
      checkEventTenantMatch(
        { metadata: { tenant_id: "tenant-a" } },
        "tenant-a",
      ),
    ).toBe(true);
    expect(
      checkEventTenantMatch(
        { metadata: { tenant_id: "tenant-b" } },
        "tenant-a",
      ),
    ).toBe(false);
    expect(checkEventTenantMatch({}, "tenant-a")).toBeNull();
  });

  it("mergeConversationTaskSnapshot protects completed conversation sync from stale running polling", () => {
    const completedSync: ConversationSyncTaskSnapshot = {
      id: "sync-1",
      status: "completed",
      source_id: "src-1",
      adapter_id: "claude",
      record_kind: "session",
      dry_run: false,
      started_at: "2026-09-04T00:00:00Z",
      finished_at: "2026-09-04T00:01:00Z",
      result: { count: 10 },
      error: null,
    };

    const staleRunningSync: ConversationSyncTaskSnapshot = {
      id: "sync-1",
      status: "running",
      source_id: "src-1",
      adapter_id: "claude",
      record_kind: "session",
      dry_run: false,
      started_at: "2026-09-04T00:00:00Z",
      finished_at: null,
      result: null,
      error: null,
    };

    const merged = mergeConversationTaskSnapshot(
      staleRunningSync,
      completedSync,
    );
    expect(merged.status).toBe("completed");
    expect(merged.finished_at).toBe("2026-09-04T00:01:00Z");
    expect(merged.result).toEqual({ count: 10 });
  });
});
