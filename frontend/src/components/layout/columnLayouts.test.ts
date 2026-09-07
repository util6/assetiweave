import { describe, expect, it } from "vitest";
import {
  fromPanelLayout,
  sanitizeColumnWeights,
  toPanelLayout,
} from "./columnLayouts";

describe("columnLayouts", () => {
  it("权重转库 layout 后保持比例", () => {
    expect(toPanelLayout([1, 2, 1])).toEqual({
      "column-0": 25,
      "column-1": 50,
      "column-2": 25,
    });
    expect(fromPanelLayout({ "column-0": 25, "column-1": 75 }, 2)).toEqual([
      25, 75,
    ]);
    expect(fromPanelLayout({ "column-0": 25 }, 2)).toBeNull();
  });

  it("handles empty or invalid inputs gracefully in toPanelLayout", () => {
    expect(toPanelLayout([])).toEqual({});
    expect(toPanelLayout([0, -1])).toEqual({
      "column-0": 50,
      "column-1": 50,
    });
  });

  it("validates fromPanelLayout with extra or missing panels", () => {
    expect(fromPanelLayout({}, 1)).toBeNull();
    expect(fromPanelLayout({ "column-0": 30, "column-1": 0 }, 2)).toBeNull();
    expect(
      fromPanelLayout({ "column-0": 30, "column-1": 70, "column-2": 100 }, 2),
    ).toEqual([30, 70]);
  });

  it("sanitizes persisted weights and rescales to fallback total", () => {
    expect(sanitizeColumnWeights([2, 1, 1], [1, 1, 1])).toEqual([
      1.5, 0.75, 0.75,
    ]);
    expect(sanitizeColumnWeights([2, 0, 1], [1, 1, 1])).toEqual([1, 1, 1]);
    expect(sanitizeColumnWeights([2, 1], [1, 1, 1])).toEqual([1, 1, 1]);
  });
});
