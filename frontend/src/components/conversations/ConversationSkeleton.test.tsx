import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import {
  ConversationLoadingState,
  ConversationPreviewLoadingState,
  ConversationsPageSkeleton,
} from "./ConversationSkeleton";

describe("ConversationSkeleton", () => {
  it("composes the page fallback from the columns recipe", () => {
    const html = renderToStaticMarkup(<ConversationsPageSkeleton label="Loading conversations" />);

    expect(html).toContain("app-skeleton-root");
    expect(html).toContain("lg:flex-row");
    expect((html.match(/<section/g) ?? []).length).toBe(3);
    expect((html.match(/role="status"/g) ?? []).length).toBe(1);
  });

  it("uses one content status root for local loading fallbacks", () => {
    const listHtml = renderToStaticMarkup(<ConversationLoadingState label="Loading session" />);
    const previewHtml = renderToStaticMarkup(<ConversationPreviewLoadingState label="Loading preview" />);

    expect((listHtml.match(/role="status"/g) ?? []).length).toBe(1);
    expect((previewHtml.match(/role="status"/g) ?? []).length).toBe(1);
    expect(listHtml).toContain("Loading session");
    expect(previewHtml).toContain("Loading preview");
  });
});
