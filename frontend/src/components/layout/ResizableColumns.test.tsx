import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import {
  ResizableColumns,
  calculateScrollThumb,
  getColumnBoundaries,
  resolveColumnMinWidths,
  resizeColumnDragWeights,
  resizeColumnWeights,
  sanitizeColumnWeights,
} from "./ResizableColumns";

describe("ResizableColumns", () => {
  it("renders accessible resize handles between columns", () => {
    const html = renderToStaticMarkup(
      <ResizableColumns
        ariaLabel="Resize columns"
        className="min-h-40"
        columns={[
          { defaultWeight: 0.72 },
          { defaultWeight: 1.14, minWidthScale: 1.1 },
          { defaultWeight: 1.4, minWidthScale: 1.45 },
        ]}
        minimumWidth={280}
        scrollBarLabel="Scroll columns"
        scrollLeftLabel="Scroll columns left"
        scrollRightLabel="Scroll columns right"
      >
        <section>Sources</section>
        <section>Skills</section>
        <section>Mount targets</section>
      </ResizableColumns>,
    );

    expect(html).toContain('role="separator"');
    expect(html).toContain('aria-orientation="vertical"');
    expect(html).toContain("Resize columns 1");
    expect(html).toContain("Resize columns 2");
    expect(html).toContain('role="scrollbar"');
    expect(html).toContain("Scroll columns left");
    expect(html).toContain("Scroll columns right");
    expect(html).toContain("--resizable-columns-template");
    expect(html).toContain("--resizable-columns-min-width");
    expect(html).toContain("--resizable-columns-width");
    expect(html).toContain("w-[max(100%,var(--resizable-columns-width))]");
  });

  it("clamps drag resizing to the neighboring column minimums", () => {
    const nextWeights = resizeColumnWeights({
      containerWidth: 900,
      deltaPx: 240,
      handleIndex: 0,
      minWidths: [240, 360, 240],
      weights: [1, 1, 1],
    });

    expect(nextWeights.map((weight) => Number(weight.toFixed(3)))).toEqual([0.8, 1.2, 1]);
  });

  it("keeps committed weights when measured minimum-width tracks cannot move", () => {
    const committedWeights = [0.72, 0.9, 1.45];
    const measuredWeights = [0.8647887323943662, 0.951267605633803, 1.2539436619718312];

    expect(
      resizeColumnDragWeights({
        committedWeights,
        containerWidth: 1704,
        deltaPx: 180,
        handleIndex: 0,
        minWidths: [480, 528, 696],
        weights: measuredWeights,
      }),
    ).toEqual(committedWeights);
  });

  it("sanitizes persisted weights before using them", () => {
    expect(sanitizeColumnWeights([2, 1, 1], [1, 1, 1])).toEqual([1.5, 0.75, 0.75]);
    expect(sanitizeColumnWeights([2, 0, 1], [1, 1, 1])).toEqual([1, 1, 1]);
    expect(sanitizeColumnWeights([2, 1], [1, 1, 1])).toEqual([1, 1, 1]);
  });

  it("rescales persisted weights to the default weight total", () => {
    const sanitizedWeights = sanitizeColumnWeights([0.3244, 0.2351, 0.4405], [0.72, 0.9, 1.45]);

    expect(Number(sanitizedWeights.reduce((sum, weight) => sum + weight, 0).toFixed(2))).toBe(3.07);
  });

  it("converts weights into cumulative handle positions", () => {
    expect(getColumnBoundaries([1, 2, 1]).map((boundary) => Number(boundary.toFixed(2)))).toEqual([0.25, 0.75]);
  });

  it("scales each column minimum width from the global setting", () => {
    expect(
      resolveColumnMinWidths(280, [
        { defaultWeight: 0.72 },
        { defaultWeight: 1.14, minWidthScale: 1.1 },
        { defaultWeight: 1.4, minWidthScale: 1.45 },
      ]),
    ).toEqual([280, 308, 406]);
  });

  it("calculates a mac-style scrollbar thumb from viewport metrics", () => {
    expect(calculateScrollThumb({ clientWidth: 900, scrollLeft: 300, scrollWidth: 1500 })).toEqual({
      leftRatio: 0.5,
      widthRatio: 0.6,
    });
  });
});
