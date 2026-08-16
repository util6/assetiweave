import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { AppSkeleton, Skeleton, SkeletonText } from "./skeleton";

describe("foundation skeleton public entry", () => {
  it("renders the primitive and text helper", () => {
    const html = renderToStaticMarkup(
      <>
        <Skeleton className="h-4" />
        <SkeletonText lines={2} />
      </>,
    );

    expect(html).toContain('aria-hidden="true"');
    expect((html.match(/aurora-skeleton/g) ?? []).length).toBe(3);
  });

  it.each(["list", "cards", "columns"] as const)("renders the %s layout", (layout) => {
    const html = renderToStaticMarkup(<AppSkeleton label="Loading" layout={layout} />);

    expect(html).toContain('aria-busy="true"');
    expect(html).toContain("Loading");
  });
});
