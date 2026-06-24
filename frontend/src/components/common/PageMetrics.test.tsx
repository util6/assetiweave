import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { PageMetrics } from "./PageMetrics";

describe("PageMetrics", () => {
  it("renders metric cards for the page header area", () => {
    const html = renderToStaticMarkup(
      <PageMetrics
        metrics={[
          { label: "来源", value: 4 },
          { label: "技能", value: 128 },
          { label: "应用", value: 8 },
        ]}
      />,
    );

    expect(html).toContain("来源");
    expect(html).toContain("128");
    expect(html).toContain("h-10");
    expect(html).toContain("min-w-[5.75rem]");
    expect(html).toContain('data-page-metric=""');
    expect(html).not.toContain("grid-cols-3");
  });
});
