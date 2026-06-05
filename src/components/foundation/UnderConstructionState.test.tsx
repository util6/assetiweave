import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { UnderConstructionState } from "./UnderConstructionState";

describe("UnderConstructionState", () => {
  it("renders the in-progress copy and optional actions", () => {
    const html = renderToStaticMarkup(
      <UnderConstructionState
        actions={<button type="button">Open roadmap</button>}
        description="This view is wired into navigation but is not ready yet."
        eyebrow="Feature in progress"
        title="MCP servers is under construction"
      />,
    );

    expect(html).toContain("Feature in progress");
    expect(html).toContain("MCP servers is under construction");
    expect(html).toContain("This view is wired into navigation but is not ready yet.");
    expect(html).toContain("Open roadmap");
  });

  it("renders a labelled static section without browser globals", () => {
    const html = renderToStaticMarkup(<UnderConstructionState title="此功能正在建设中" />);

    expect(html).toContain("<section");
    expect(html).toContain("aria-labelledby=");
    expect(html).toContain("此功能正在建设中");
  });
});
