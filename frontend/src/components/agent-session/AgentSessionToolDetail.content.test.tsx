/* @vitest-environment jsdom */

import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { I18nProvider } from "../../i18n/I18nProvider";
import type {
  AgentSessionItemView,
  ToolContentBlock,
} from "../../types/agentSession";
import { AgentSessionToolDetail } from "./AgentSessionToolDetail";

function renderDetail(item: AgentSessionItemView) {
  return render(
    <I18nProvider>
      <AgentSessionToolDetail item={item} />
    </I18nProvider>,
  );
}

describe("AgentSessionToolDetail Content Blocks (T06)", () => {
  it("maps ACP/tool content with Diff, path, and location to typed blocks with home abbreviation", () => {
    const diffBlock: ToolContentBlock = {
      type: "diff",
      path: "/Users/developer/code-space/assetiweave/src/main.ts",
      oldText: "const a = 1;\n",
      newText: "const a = 2;\n",
      unifiedDiff: null,
    };
    const locationBlock: ToolContentBlock = {
      type: "location",
      path: "/Users/developer/code-space/assetiweave/src/index.ts",
      line: 42,
      column: 8,
    };

    const item: AgentSessionItemView = {
      id: "tool-content-1",
      kind: "tool",
      sequence: 1,
      delivery: "live",
      state: "succeeded",
      toolName: "edit_file",
      toolInput: [locationBlock],
      toolOutput: [diffBlock],
    };

    renderDetail(item);

    // Location block should render with abbreviated path and line:column
    expect(
      screen.getByTestId("agent-session-tool-location-tool-content-1"),
    ).toBeTruthy();
    expect(
      screen.getByText(/~\/code-space\/assetiweave\/src\/index\.ts:42:8/),
    ).toBeTruthy();

    // Diff block should render with abbreviated path
    expect(
      screen.getByTestId("agent-session-tool-diff-tool-content-1"),
    ).toBeTruthy();
    expect(
      screen.getAllByText(/~\/code-space\/assetiweave\/src\/main\.ts/).length,
    ).toBeGreaterThanOrEqual(1);
  });

  it("handles diff missing one side gracefully without error and handles truncated diff", () => {
    const newFileDiff: ToolContentBlock = {
      type: "diff",
      path: "/home/developer/new-file.txt",
      oldText: null,
      newText: "Hello world\n",
      unifiedDiff: null,
    };
    const deletedFileDiff: ToolContentBlock = {
      type: "diff",
      path: "/home/developer/deleted-file.txt",
      oldText: "Old content\n",
      newText: null,
      unifiedDiff: null,
      isTruncated: true,
    };

    const item: AgentSessionItemView = {
      id: "tool-diff-sides",
      kind: "tool",
      sequence: 2,
      delivery: "live",
      state: "succeeded",
      toolName: "apply_patch",
      toolOutput: [newFileDiff, deletedFileDiff],
    };

    renderDetail(item);

    // Both diffs render without crashing
    expect(
      screen.getAllByText(/~\/new-file\.txt/).length,
    ).toBeGreaterThanOrEqual(1);
    expect(
      screen.getAllByText(/~\/deleted-file\.txt/).length,
    ).toBeGreaterThanOrEqual(1);

    // Truncated diff indicates truncation and avoids fake exact counts
    expect(
      screen.getByTestId("agent-session-diff-truncated-tool-diff-sides-1"),
    ).toBeTruthy();
    expect(screen.getByText(/truncated/i)).toBeTruthy();
  });

  it("renders image block with path, alt, mimeType safely within lane bounds", () => {
    const imageBlock: ToolContentBlock = {
      type: "image",
      path: "/Users/developer/screenshots/output.png",
      mimeType: "image/png",
      alt: "Generated UI mockup",
    };

    const item: AgentSessionItemView = {
      id: "tool-image-1",
      kind: "tool",
      sequence: 3,
      delivery: "live",
      state: "succeeded",
      toolName: "generate_image",
      toolOutput: [imageBlock],
    };

    renderDetail(item);

    const imageElement = screen.getByRole("img", {
      name: "Generated UI mockup",
    });
    expect(imageElement).toBeTruthy();
    expect(imageElement.getAttribute("alt")).toBe("Generated UI mockup");
    // Lane width should be constrained
    expect(imageElement.className).toContain("max-w-full");
    // Displays abbreviated path
    expect(screen.getByText(/~\/screenshots\/output\.png/)).toBeTruthy();
  });

  it("renders artifact block using renderer registry metadata without executing scripts", () => {
    const artifactBlock: ToolContentBlock = {
      type: "artifact",
      artifactId: "art-report-01",
      renderer: "markdown",
      title: "Performance Report",
    };

    const item: AgentSessionItemView = {
      id: "tool-artifact-1",
      kind: "tool",
      sequence: 4,
      delivery: "live",
      state: "succeeded",
      toolName: "create_artifact",
      toolOutput: [artifactBlock],
    };

    renderDetail(item);

    expect(
      screen.getByTestId("agent-session-tool-artifact-tool-artifact-1"),
    ).toBeTruthy();
    expect(screen.getByText("Performance Report")).toBeTruthy();
    expect(screen.getByText(/markdown/i)).toBeTruthy();
    expect(screen.getByText(/art-report-01/)).toBeTruthy();
  });

  it("renders unknown provider content with type and bounded text without crashing", () => {
    const unknownBlock: ToolContentBlock = {
      type: "unknown",
      providerType: "mcp.custom_resource_preview",
      display: '{"customField": "customValue", "depth": 42}',
    };

    const item: AgentSessionItemView = {
      id: "tool-unknown-1",
      kind: "tool",
      sequence: 5,
      delivery: "live",
      state: "succeeded",
      toolName: "custom_tool",
      toolOutput: [unknownBlock],
    };

    renderDetail(item);

    const unknownElem = screen.getByTestId(
      "agent-session-tool-unknown-tool-unknown-1",
    );
    expect(unknownElem).toBeTruthy();
    expect(screen.getByText(/mcp\.custom_resource_preview/)).toBeTruthy();
    expect(screen.getByText(/customField/)).toBeTruthy();
    // Bounded max height and overflow
    expect(unknownElem.className).toContain("overflow-auto");
  });

  it("extracts diff and file objects from ACP/legacy payloads automatically", () => {
    // ACP payload style: write_file or patch result
    const item: AgentSessionItemView = {
      id: "tool-acp-legacy",
      kind: "tool",
      sequence: 6,
      delivery: "live",
      state: "succeeded",
      toolName: "write_file",
      toolInput: {
        path: "/Users/developer/project/file.txt",
        content: "updated content",
      },
      toolOutput: {
        file_diff:
          "--- a/file.txt\n+++ b/file.txt\n@@ -1 +1 @@\n-old\n+updated content\n",
        file_name: "/Users/developer/project/file.txt",
      },
    };

    renderDetail(item);

    expect(
      screen.getAllByText(/~\/project\/file\.txt/).length,
    ).toBeGreaterThanOrEqual(1);
    expect(
      screen.getAllByText(/updated content/).length,
    ).toBeGreaterThanOrEqual(1);
  });
});
