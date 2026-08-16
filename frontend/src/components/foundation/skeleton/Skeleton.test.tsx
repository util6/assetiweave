import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import {
  AppSkeleton,
  Skeleton,
  SkeletonBoundary,
  SkeletonColumn,
  SkeletonText,
  skeletonRecipes,
} from "./index";

describe("skeleton primitives", () => {
  it("forces primitives to be hidden from assistive technology", () => {
    const html = renderToStaticMarkup(<Skeleton className="h-4" />);

    expect(html).toContain('aria-hidden="true"');
    expect(html).toContain("h-4");
  });

  it("renders three text lines by default and normalizes zero lines", () => {
    expect((renderToStaticMarkup(<SkeletonText />).match(/aurora-skeleton/g) ?? []).length).toBe(3);
    expect((renderToStaticMarkup(<SkeletonText lines={0} />).match(/aurora-skeleton/g) ?? []).length).toBe(1);
  });
});

describe("skeleton recipes", () => {
  it.each(Object.entries(skeletonRecipes))("renders a default %s recipe", (_name, definition) => {
    const Recipe = definition.component;
    const html = renderToStaticMarkup(<Recipe {...definition.defaults} />);

    expect(html).toContain("aurora-skeleton");
    expect((html.match(/aurora-skeleton/g) ?? []).length).toBeLessThanOrEqual(80);
  });
});

describe("AppSkeleton", () => {
  it.each(["list", "cards", "columns"] as const)("renders %s through the common entry point", (layout) => {
    const html = renderToStaticMarkup(<AppSkeleton label="Loading" layout={layout} />);

    expect(html).toContain('aria-busy="true"');
    expect(html).toContain('role="status"');
    expect(html).toContain("Loading");
    expect((html.match(/role="status"/g) ?? []).length).toBe(1);
    expect((html.match(/aurora-skeleton/g) ?? []).length).toBeLessThanOrEqual(80);
  });

  it("does not render page chrome for content scope", () => {
    const html = renderToStaticMarkup(<AppSkeleton label="Loading" layout="cards" scope="content" />);

    expect(html).toContain("app-skeleton-root");
    expect(html).not.toContain("w-64");
  });

  it("uses custom children instead of default recipe content", () => {
    const html = renderToStaticMarkup(
      <AppSkeleton label="Loading" layout="columns" layoutProps={{ columns: 3 }}>
        <SkeletonColumn>
          <Skeleton className="feature-only" />
        </SkeletonColumn>
      </AppSkeleton>,
    );

    expect(html).toContain("feature-only");
    expect(html).not.toContain("w-3/4");
  });

  it("requires a non-empty label", () => {
    expect(() => renderToStaticMarkup(<AppSkeleton label=" " layout="list" />)).toThrow(
      "AppSkeleton requires a non-empty label",
    );
  });
});

describe("SkeletonBoundary", () => {
  it("switches exclusively between fallback and real content", () => {
    const loadingHtml = renderToStaticMarkup(
      <SkeletonBoundary fallbackChildren={<span>fallback</span>} label="Loading" layout="list" loading>
        <span>content</span>
      </SkeletonBoundary>,
    );
    const contentHtml = renderToStaticMarkup(
      <SkeletonBoundary fallbackChildren={<span>fallback</span>} label="Loading" layout="list" loading={false}>
        <span>content</span>
      </SkeletonBoundary>,
    );

    expect(loadingHtml).toContain("fallback");
    expect(loadingHtml).not.toContain("content");
    expect(contentHtml).toContain("content");
    expect(contentHtml).not.toContain("fallback");
  });
});
