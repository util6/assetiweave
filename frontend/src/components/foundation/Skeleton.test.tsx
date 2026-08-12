import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import { PageSkeleton, type PageSkeletonKind } from "./Skeleton";

describe("PageSkeleton", () => {
  const cases: Array<[PageSkeletonKind, string[]]> = [
    ["catalog", ["--app-page-x", "rounded-2xl", "h-10", "min-h-[5rem]"]],
    ["sources", ["--app-page-x", "min-h-[6.5rem]"]],
    ["groups", ["grid-cols-1 lg:grid-cols-3", "sticky"]],
    ["mounts", ["grid-cols-1 lg:grid-cols-3", "sticky"]],
    ["conversations", ["w-24", "grid-cols-1 lg:grid-cols-3"]],
    ["web-records", ["grid-cols-1 lg:grid-cols-2", "h-14"]],
    ["prompts", ["rounded-[2rem]", "min-h-[16rem]"]],
    ["memory-library", ["grid-cols-1 lg:grid-cols-2", "h-14"]],
    ["memory-overview", ["xl:grid-cols-2", "min-h-[10rem]"]],
    ["memory-dreams", ["minmax(20rem,0.8fr)", "h-48"]],
    ["memory-recall", ["minmax(21rem,0.75fr)", "grid-cols-2"]],
    ["manual", ["max-w-5xl", "h-44"]],
  ];

  it.each(cases)("renders a real %s page structure instead of a generic panel", (kind, markers) => {
    const html = renderToStaticMarkup(<PageSkeleton kind={kind} label="Loading" />);

    expect(html).toContain('aria-busy="true"');
    expect(html).toContain("Loading");
    markers.forEach((marker) => expect(html).toContain(marker));
  });
});
