import { describe, expect, it } from "vitest";
import {
  checkEventTenantMatch,
  isTerminalStatus,
  mergeTaskSnapshot,
} from "./taskMergeUtils";
import { mergeConversationTaskSnapshot } from "./ConversationSyncProvider";
import { mergeTeamRunTasks } from "./TeamTaskProvider";
import type { SourceScanTaskSnapshot } from "../../services/catalog";
import type { ConversationSyncTaskSnapshot } from "../../services/conversations";
import type { TeamRuntimeTaskSnapshot } from "../../types/team";

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
    const completedTask: SourceScanTaskSnapshot = {
      id: "task-1",
      status: "completed",
      scope: "all",
      kind: null,
      progress: {
        phase: "completed",
        completed_source_count: 10,
        total_source_count: 10,
        current_source_name: null,
      },
      started_at: "2026-09-04T00:00:00Z",
      finished_at: "2026-09-04T00:02:00Z",
      result: [],
      error: null,
    };

    const stalePollingRunningTask: SourceScanTaskSnapshot = {
      id: "task-1",
      status: "running",
      scope: "all",
      kind: null,
      progress: {
        phase: "scanning",
        completed_source_count: 5,
        total_source_count: 10,
        current_source_name: "source-a",
      },
      started_at: "2026-09-04T00:00:00Z",
      finished_at: null,
      result: null,
      error: null,
    };

    const merged = mergeTaskSnapshot(stalePollingRunningTask, completedTask);
    expect(merged.status).toBe("completed");
    expect(merged.finished_at).toBe("2026-09-04T00:02:00Z");
    expect(merged.progress.completed_source_count).toBe(10);
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

  it("mergeTeamRunTasks preserves completed state when stale task array arrives", () => {
    const completedTeamTask: TeamRuntimeTaskSnapshot = {
      task_id: "team-1",
      kind: "TeamRun",
      dedup_key: "key-1",
      state: "Succeeded",
      progress: null,
      error: null,
      started_at: "2026-09-04T00:00:00Z",
      finished_at: "2026-09-04T00:01:00Z",
      result: null,
      detail: null,
    };

    const staleRunningTeamTask: TeamRuntimeTaskSnapshot = {
      task_id: "team-1",
      kind: "TeamRun",
      dedup_key: "key-1",
      state: "Running",
      progress: null,
      error: null,
      started_at: "2026-09-04T00:00:00Z",
      finished_at: null,
      result: null,
      detail: null,
    };

    const merged = mergeTeamRunTasks(
      [completedTeamTask],
      [staleRunningTeamTask],
    );
    expect(merged).toHaveLength(1);
    expect(merged[0].state).toBe("Succeeded");
    expect(merged[0].finished_at).toBe("2026-09-04T00:01:00Z");
  });
});
