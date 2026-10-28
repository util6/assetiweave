/* @vitest-environment jsdom */
import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it, vi } from "vitest";
import { MarkdownContent, parseAiwcSessionUri } from "./ConversationMarkdown";

describe("ConversationMarkdown", () => {
  it("renders markdown headings, lists, inline code, strong text, and code fences", () => {
    const html = renderToStaticMarkup(
      <MarkdownContent
        value={[
          "# Question summary",
          "",
          "Use `conversation.sync` with **dry run** first.",
          "",
          "- Session",
          "- Question",
          "",
          "```sh",
          "assetiweave-cli conversation sync --dry-run",
          "```",
        ].join("\n")}
      />,
    );

    expect(html).toContain("Question summary");
    expect(html).toContain("<code");
    expect(html).toContain("conversation.sync");
    expect(html).toContain("<strong");
    expect(html).toContain("dry run");
    expect(html).toContain("<li>Session</li>");
    expect(html).toContain("assetiweave-cli conversation sync --dry-run");
  });

  it("renders markdown tables and mermaid diagrams", () => {
    const html = renderToStaticMarkup(
      <MarkdownContent
        value={[
          "| 阶段 | 状态 |",
          "| --- | --- |",
          "| 导入 | 完成 |",
          "| 渲染 | 等待 |",
          "",
          "```mermaid",
          "flowchart TD",
          "  A[导入] --> B[渲染]",
          "```",
        ].join("\n")}
      />,
    );

    expect(html).toContain("<table");
    expect(html).toContain("<th");
    expect(html).toContain("阶段");
    expect(html).toContain("<td");
    expect(html).toContain("完成");
    expect(html).toContain('data-mermaid-diagram="true"');
    expect(html).toContain("flowchart TD");
  });

  it("GFM 表格与行内代码实际渲染", () => {
    const { container } = render(
      <MarkdownContent value={"| A | B |\n| --- | --- |\n| `x` | y |"} />,
    );
    expect(screen.getByRole("table")).toBeTruthy();
    expect(container.querySelector("td code")?.textContent).toBe("x");
  });

  it("内容中的脚本不成为可执行 DOM", () => {
    const { container } = render(
      <MarkdownContent value={"<script>window.fixture = 1</script>"} />,
    );
    expect(container.querySelector("script")).toBeNull();
  });

  it("renders diff and patch fences as unified code changes", () => {
    const html = renderToStaticMarkup(
      <MarkdownContent
        value={[
          "```diff",
          "diff --git a/frontend/src/App.tsx b/frontend/src/App.tsx",
          "--- a/frontend/src/App.tsx",
          "+++ b/frontend/src/App.tsx",
          "@@ -10,2 +10,2 @@ export function App() {",
          "-  return <OldView />;",
          "+  return <NewView />;",
          " }",
          "```",
        ].join("\n")}
      />,
    );

    expect(html).toContain('data-conversation-diff="unified"');
    expect(html).toContain('data-diff-file="frontend/src/App.tsx"');
    expect(html).toContain("diff-code-delete");
    expect(html).toContain("diff-code-insert");
    expect(html).toContain("OldView");
    expect(html).toContain("NewView");
  });

  it("renders inline and display LaTeX math in markdown previews", () => {
    const html = renderToStaticMarkup(
      <MarkdownContent
        value={[
          "Inline math supports \\(\\alpha + \\beta\\) and $E=mc^2$.",
          "",
          "$$",
          "\\frac{a}{b} = c",
          "$$",
          "",
          "\\[",
          "\\int_0^1 x^2 dx",
          "\\]",
        ].join("\n")}
      />,
    );

    expect(html.match(/data-latex-math="inline"/g)).toHaveLength(2);
    expect(html.match(/data-latex-math="display"/g)).toHaveLength(2);
    expect(html).toContain("katex");
    expect(html).not.toContain("\\(\\alpha");
    expect(html).not.toContain("$$");
  });

  it("renders quotes, links with security attributes, and empty string safely", () => {
    const html = renderToStaticMarkup(
      <MarkdownContent
        value={[
          "> A blockquote note",
          "",
          "Check out [AssetIWeave](https://github.com/util6/assetiweave).",
        ].join("\n")}
      />,
    );

    expect(html).toContain("<blockquote");
    expect(html).toContain("A blockquote note");
    expect(html).toContain('href="https://github.com/util6/assetiweave"');
    expect(html).toContain('target="_blank"');
    expect(html).toContain('rel="noreferrer"');

    const emptyHtml = renderToStaticMarkup(<MarkdownContent value="" />);
    expect(emptyHtml).toBeDefined();
  });

  it("renders invalid mermaid gracefully without polluting document.body", async () => {
    const legacyDangling = document.createElement("div");
    legacyDangling.id = "dconversation-mermaid-legacy-dirty";
    legacyDangling.textContent = "Syntax error in text";
    document.body.appendChild(legacyDangling);

    const { container } = render(
      <MarkdownContent
        value={[
          "```mermaid",
          "flowchart TD",
          "  A --> |syntax error [broken",
          "```",
        ].join("\n")}
      />,
    );

    await waitFor(() => {
      const diagramEl = container.querySelector(
        '[data-mermaid-diagram="true"]',
      );
      expect(diagramEl).not.toBeNull();
      expect(diagramEl?.tagName.toLowerCase()).toBe("pre");
      expect(diagramEl?.textContent).toContain("syntax error");

      expect(document.body.textContent).not.toContain("Syntax error in text");
      expect(
        document.getElementById("dconversation-mermaid-legacy-dirty"),
      ).toBeNull();
      expect(
        document.querySelectorAll('[id^="dconversation-mermaid-"]'),
      ).toHaveLength(0);
    });
  });

  it("parses aiwc://session URI correctly and triggers onNavigateSession on click", () => {
    expect(parseAiwcSessionUri("aiwc://session/38623279/turn/2")).toEqual({
      sessionId: "38623279",
      turnIndex: 2,
    });
    expect(parseAiwcSessionUri("aiwc://session/abcd1234")).toEqual({
      sessionId: "abcd1234",
      turnIndex: undefined,
    });
    expect(parseAiwcSessionUri("https://example.com")).toBeNull();

    const onNavigate = vi.fn();
    const { container } = render(
      <MarkdownContent
        onNavigateSession={onNavigate}
        value="- [38623279#T2](aiwc://session/38623279/turn/2): task detail"
      />,
    );

    const link = container.querySelector("a");
    expect(link).not.toBeNull();
    fireEvent.click(link!);

    expect(onNavigate).toHaveBeenCalledWith({
      record_kind: "session",
      source_id: null,
      session_id: "38623279",
      question_id: null,
      turn_id: "2",
      part_id: null,
      block_id: null,
    });
  });
});
