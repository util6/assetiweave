import { describe, expect, it } from "vitest";
import { clampLogLineLimit, filterLogContent } from "./logViewer";

describe("log viewer utilities", () => {
  it("clamps line limits to the source component range", () => {
    expect(clampLogLineLimit(Number.NaN)).toBe(200);
    expect(clampLogLineLimit(8)).toBe(20);
    expect(clampLogLineLimit(120.6)).toBe(121);
    expect(clampLogLineLimit(9000)).toBe(5000);
  });

  it("keeps full multiline log entries when filtering by level", () => {
    const content = [
      "2026-06-01T10:00:00+08:00 INFO boot started",
      "detail line for info",
      "2026-06-01T10:00:01+08:00 WARN disk almost full",
      "warn continuation",
      "2026-06-01T10:00:02+08:00 ERROR failed to sync",
      "stack line 1",
      "stack line 2",
    ].join("\n");

    expect(filterLogContent(content, "WARN")).toBe(
      ["2026-06-01T10:00:01+08:00 WARN disk almost full", "warn continuation"].join("\n"),
    );
    expect(filterLogContent(content, "ERROR")).toBe(
      ["2026-06-01T10:00:02+08:00 ERROR failed to sync", "stack line 1", "stack line 2"].join("\n"),
    );
  });
});
