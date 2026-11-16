import { describe, expect, it } from "vitest";
import {
  inferRowGap,
  intervalCoverage,
  SCROLL_JUMP_RATIOS,
} from "./scrollCoverage";

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

describe("intentional row gap inference", () => {
  const row = (top: number, height = 132) => ({
    top,
    bottom: top + height,
    height,
  });

  it("reads the pitch gap that the collection does not declare in CSS", () => {
    // 虚拟化列表的 12px 间距来自 virtualizer 的 gap prop，集合本身没有 row-gap。
    expect(inferRowGap([row(0), row(144), row(288), row(432)])).toBe(12);
  });

  it("returns 0 for contiguous rows and for unusable input", () => {
    expect(inferRowGap([row(0), row(132), row(264)])).toBe(0);
    expect(inferRowGap([row(0)])).toBe(0);
    expect(inferRowGap([])).toBe(0);
  });

  it("does not mistake a missing row for an intentional gap", () => {
    // 缺行造成的空洞是一整个行距，必须继续算作未覆盖像素。
    expect(inferRowGap([row(0), row(144), row(288), row(576), row(720)])).toBe(
      12,
    );
    expect(inferRowGap([row(0), row(288), row(576)])).toBe(0);
  });
});
