/* @vitest-environment jsdom */

import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { RenderingStressHarness } from "./RenderingStressHarness";

describe("RenderingStressHarness", () => {
  it("only renders its fixture through a caller-supplied development/test render path", () => {
    render(
      <RenderingStressHarness render={(question) => (
        <output data-turn-count={question.turns.length}>stress fixture</output>
      )} />,
    );

    expect(screen.getByText("stress fixture").getAttribute("data-turn-count")).toBe("80");
  });
});
