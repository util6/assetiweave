import { describe, expect, it } from "vitest";
import { intervalCoverage, SCROLL_JUMP_RATIOS } from "./scrollCoverage";

describe("viewport interval union", () => {
  it("rejects collapsed or invalid viewport intervals", () => {
    expect(intervalCoverage(0, 0, [[0, 100]])).toBe(0);
    expect(intervalCoverage(100, 10, [[0, 100]])).toBe(0);
    expect(intervalCoverage(0, Infinity, [[0, 100]])).toBe(0);
  });
  it("detects partial holes even when there are visible rows", () => {
    expect(
      intervalCoverage(0, 100, [
        [0, 20],
        [80, 100],
      ]),
    ).toBe(0.4);
    expect(
      intervalCoverage(0, 100, [
        [-20, 50],
        [30, 130],
      ]),
    ).toBe(1);
    expect(intervalCoverage(0, 100, [])).toBe(0);
  });
  it("uses the same sixteen jumps in both directions", () => {
    expect(SCROLL_JUMP_RATIOS).toHaveLength(16);
    expect(SCROLL_JUMP_RATIOS).toContain(0);
    expect(SCROLL_JUMP_RATIOS).toContain(1);
  });
});
