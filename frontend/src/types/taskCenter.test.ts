import { describe, expect, it } from "vitest";
import { normalizeTaskView } from "./taskCenter";

describe("normalizeTaskView", () => {
  it("normalizes backend TaskSkippedReasonView (camelCase & snake_case) and preserves backend kind", () => {
    const view = normalizeTaskView({
      id: "task-1",
      kind: "conversationusagescan",
      title: "用量统计扫描",
      tenantId: "default",
      state: "succeeded",
      outcome: "success",
      startedAt: "2026-11-13T00:00:00Z",
      updatedAt: "2026-11-13T00:01:00Z",
      finishedAt: "2026-11-13T00:01:00Z",
      stages: [
        {
          id: "persist",
          name: "写入汇总",
          status: "succeeded",
          metrics: [],
          failures: [],
          skipped: [
            {
              reasonCode: "unchanged_hash",
              count: 4,
              samples: ["a.jsonl", "b.jsonl"],
            },
            {
              reason_code: "missing_usage",
              count: 1,
              samples: ["c.jsonl"],
            },
          ],
          steps: [],
        },
      ],
      metrics: [],
      failures: [],
      capabilities: { cancellable: false, retryable: false, clearable: true },
      revision: 3,
      changeClass: "structural",
    });

    expect(view.kind).toBe("conversationusagescan");
    expect(view.stages[0].skipped).toEqual([
      {
        reasonCode: "unchanged_hash",
        reason_code: "unchanged_hash",
        count: 4,
        samples: ["a.jsonl", "b.jsonl"],
      },
      {
        reasonCode: "missing_usage",
        reason_code: "missing_usage",
        count: 1,
        samples: ["c.jsonl"],
      },
    ]);
  });
});
