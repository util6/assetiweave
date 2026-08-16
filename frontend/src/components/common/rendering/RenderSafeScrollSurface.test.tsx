/* @vitest-environment jsdom */

import { render, screen } from "@testing-library/react";
import type { RefObject } from "react";
import { existsSync, readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";
import { RenderSafeScrollSurface } from "./RenderSafeScrollSurface";

describe("RenderSafeScrollSurface", () => {
  it("forwards the scroll element ref and preserves content", () => {
    const ref = { current: null } as RefObject<HTMLDivElement | null>;

    render(
      <RenderSafeScrollSurface className="custom-scroll-surface" ref={ref}>
        <span>scroll content</span>
      </RenderSafeScrollSurface>,
    );

    const element = screen.getByText("scroll content").parentElement;
    expect(element).toHaveProperty("dataset.renderSafeScrollSurface", "");
    expect(element?.className).toContain("render-safe-scroll-surface");
    expect(element?.className).toContain("custom-scroll-surface");
    expect(ref.current).toBe(element);
  });

  it("keeps the scroll surface and content rules opaque and isolated", () => {
    const cssPath = [resolve(process.cwd(), "frontend/src/styles/index.css"), resolve(process.cwd(), "src/styles/index.css")].find(existsSync);
    expect(cssPath).toBeTruthy();
    const css = readFileSync(cssPath!, "utf8");
    const surfaceRule = css.match(/\.render-safe-scroll-surface\s*\{([^}]+)\}/)?.[1] ?? "";
    const contentRule = css.match(/\.render-safe-scroll-content\s*\{([^}]+)\}/)?.[1] ?? "";

    expect(surfaceRule).toContain("background: rgb(var(--color-background))");
    expect(surfaceRule).toContain("isolation: isolate");
    expect(surfaceRule).toContain("contain: paint");
    expect(surfaceRule).not.toContain("backdrop-filter");
    expect(surfaceRule).not.toMatch(/\/\s*0\./);
    expect(contentRule).toContain("background: rgb(var(--color-background))");
    expect(contentRule).not.toContain("backdrop-filter");
    expect(contentRule).not.toMatch(/\/\s*0\./);
  });
});
