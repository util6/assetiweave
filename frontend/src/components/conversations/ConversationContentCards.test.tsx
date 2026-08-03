/* @vitest-environment jsdom */

import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { StrictMode } from "react";
import { renderToStaticMarkup } from "react-dom/server";
import { afterEach, describe, expect, it, vi } from "vitest";
import {
  ConversationContentCards,
  buildConversationContentBlocks,
  buildConversationDisplayNodes,
  conversationCardColor,
} from "./ConversationContentCards";
import type { Translator } from "../../i18n/I18nProvider";
import { messages, type TranslationParams } from "../../i18n/messages";
import type { ConversationCard, ConversationContentNode, ConversationPart } from "../../types";

const revealPath = vi.hoisted(() => vi.fn());
vi.mock("../../services/catalog", () => ({ revealPath }));

describe("ConversationContentCards", () => {
  afterEach(() => {
    cleanup();
  });

  it("does not infer card types for undeclared parts", () => {
    const blocks = buildConversationContentBlocks([
      {
        id: "part-tool-call",
        turn_id: "turn-1",
        part_index: 0,
        role: "tool",
        kind: "tool",
        text: "function_call: update_plan",
        metadata_json: JSON.stringify({
          name: "update_plan",
          type: "function_call",
        }),
      },
    ]);

    expect(blocks).toEqual([]);
  });

  it("uses adapter-declared content card metadata", () => {
    const blocks = buildConversationContentBlocks([
      {
        id: "part-declared",
        turn_id: "turn-1",
        part_index: 0,
        role: "tool",
        kind: "tool",
        text: "## Declared result\n\nAdapter controls this card.",
        metadata_json: JSON.stringify({
          content_card: {
            type: "result",
            format: "markdown",
            suffix: "declared-result",
          },
        }),
      },
    ]);

    expect(blocks).toHaveLength(1);
    expect(blocks[0]).toMatchObject({
      format: "markdown",
      id: "part-declared-declared-result",
      type: "result",
    });

    const html = renderToStaticMarkup(
      <ConversationContentCards
        blocks={blocks}
        t={t}
        visibility={{
          answer: true,
          code: true,
          command: true,
          result: true,
          tool: true,
        }}
      />,
    );

    expect(html).toContain('data-result-format="markdown"');
    expect(html).toContain("Declared result");
  });

  it("renders an unknown namespaced kind through its Core renderer and stable card id", () => {
    const blocks = buildConversationContentBlocks([], [{
      card_id: "conversation-part-stable",
      part_id: "conversation-part-stable",
      adapter_id: "claude-code",
      kind: "claude-code.reasoning",
      semantic_role: "reasoning",
      renderer: "markdown",
      role: "assistant",
      body: "## Adapter reasoning",
      translated_body: null,
      legacy_anchor_ids: ["conversation-part-stable-reasoning"],
    }]);

    expect(blocks[0]).toMatchObject({
      id: "conversation-part-stable",
      legacyAnchorIds: ["conversation-part-stable-reasoning"],
      renderer: "markdown",
      type: "claude-code.reasoning",
    });
    expect(conversationCardColor("claude-code.reasoning", {})).toMatch(/^#[0-9a-f]{6}$/);

    const html = renderToStaticMarkup(
      <ConversationContentCards
        activeBlockId="conversation-part-stable-reasoning"
        blocks={blocks}
        colors={{}}
        t={t}
        visibility={{ answer: true }}
      />,
    );

    expect(html).toContain('id="conversation-card-conversation-part-stable"');
    expect(html).toContain("Reasoning");
    expect(html).toContain("Adapter reasoning");
    expect(html).toContain("ring-2");
  });

  it("renders a path card as a clickable local path", async () => {
    revealPath.mockResolvedValue(undefined);
    const skillPath = "/Users/test/.codex/skills/session-exporter/SKILL.md";
    const blocks = buildConversationContentBlocks([], [{
      card_id: "conversation-part-skill",
      part_id: "conversation-part-skill",
      adapter_id: "codex",
      kind: "codex.skill",
      semantic_role: "skill",
      renderer: "path",
      role: "system",
      body: skillPath,
      legacy_anchor_ids: [],
    }]);

    render(
      <ConversationContentCards
        blocks={blocks}
        t={t}
        translationAvailabilityChecker={async () => ({
          available: false,
          error: null,
          version: null,
        })}
        visibility={{}}
      />,
    );

    const pathButton = screen.getByRole("button", { name: "在文件管理器中显示 Skill 路径" });
    expect(pathButton.textContent).toContain("~/.codex/skills/session-exporter/SKILL.md");
    fireEvent.click(pathButton);
    await waitFor(() => expect(revealPath).toHaveBeenCalledWith(skillPath));
  });

  it("collapses namespaced built-in semantics to the existing card presentation type", () => {
    const blocks = buildConversationContentBlocks([], [{
      card_id: "conversation-part-answer",
      part_id: "conversation-part-answer",
      adapter_id: "claude-code",
      kind: "claude-code.answer",
      semantic_role: "answer",
      renderer: "markdown",
      role: "assistant",
      body: "Canonical answer",
      legacy_anchor_ids: [],
    }]);

    expect(blocks[0]).toMatchObject({
      kind: "claude-code.answer",
      type: "answer",
    });
  });

  it("shows the derived id fragment for every card type while retaining full block ids", () => {
    const partId = "conversation-part-1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef";
    const cardTypes = ["answer", "tool", "command", "code", "result"] as const;
    const blocks = buildConversationContentBlocks(cardTypes.map((cardType, index) => (
      declaredPart(`${partId}-${index}`, cardType, `Fragment-targeted ${cardType}`)
    )));

    const html = renderToStaticMarkup(
      <ConversationContentCards
        blocks={blocks}
        t={t}
        visibility={{
          answer: true,
          code: true,
          command: true,
          result: true,
          tool: true,
        }}
      />,
    );

    expect(html.match(/>12345678</g)).toHaveLength(cardTypes.length);
    cardTypes.forEach((cardType, index) => {
      expect(html).toContain(`data-conversation-card-id="${partId}-${index}-${cardType}"`);
    });
  });

  it("does not render protocol metadata as card body", () => {
    const blocks = buildConversationContentBlocks([
      {
        id: "part-metadata-only",
        turn_id: "turn-1",
        part_index: 0,
        role: "tool",
        kind: "tool",
        metadata_json: JSON.stringify({
          content_card: {
            type: "tool",
          },
          name: "update_plan",
        }),
      },
    ]);

    expect(blocks).toEqual([]);
  });

  it("filters adapter browse truncation markers from every declared card type", () => {
    const marker = "[AssetIWeave adapter truncated 10363 characters for browsing.]";
    const blocks = buildConversationContentBlocks([
      declaredPart("part-answer-marker", "answer", marker),
      declaredPart("part-tool-marker", "tool", marker),
      declaredPart("part-command-marker", "command", marker),
      declaredPart("part-code-marker", "code", marker),
      declaredPart("part-result-marker", "result", marker),
      declaredPart("part-result-useful", "result", `useful result\n\n${marker}`),
    ]);

    expect(blocks).toHaveLength(1);
    expect(blocks[0]).toMatchObject({
      id: "part-result-useful-result",
      text: "useful result",
      type: "result",
    });
  });

  it("keeps adapter-declared command output as one plain result", () => {
    const blocks = buildConversationContentBlocks([
      commandPart(),
      resultPart([
        "Chunk ID: 0e43bd",
        "Wall time: 0.0000 seconds",
        "Process exited with code 0",
        "Original token count: 2387",
        "Output:",
        'import { invoke } from "@tauri-apps/api/core";',
        "import type {",
        "  ConversationAdapter,",
        "  ConversationMutationResult,",
        "} from \"../types\";",
        "",
        "export interface ConversationSessionListParams {",
        "  adapter_id?: string | null;",
        "}",
      ].join("\n")),
    ]);

    expect(blocks.map((block) => block.type)).toEqual(["command", "result"]);
    expect(blocks[1]).toMatchObject({
      id: "part-command-result",
      type: "result",
    });
    expect(blocks[1].text).toContain("Output:");
    expect(blocks[1].text).toContain('import { invoke } from "@tauri-apps/api/core";');
  });

  it("builds Execution parents from backend indices without rematching interleaved results", () => {
    const cards: ConversationCard[] = [
      projectedCard("command-a", "command", "pnpm typecheck"),
      projectedCard("command-b", "command", "pnpm test"),
      projectedCard("result-b", "result", "tests passed"),
      projectedCard("result-a", "result", "types passed"),
    ];
    const nodes: ConversationContentNode[] = [
      {
        type: "execution",
        turn_id: "turn-1",
        source_execution_id: "call-a",
        command_card_index: 0,
        result_card_indices: [3],
      },
      {
        type: "execution",
        turn_id: "turn-1",
        source_execution_id: "call-b",
        command_card_index: 1,
        result_card_indices: [2],
      },
    ];

    const displayNodes = buildConversationDisplayNodes(cards, nodes);

    expect(displayNodes).toEqual([
      {
        type: "execution",
        turnId: "turn-1",
        sourceExecutionId: "call-a",
        command: expect.objectContaining({ id: "command-a", text: "pnpm typecheck" }),
        results: [expect.objectContaining({ id: "result-a", text: "types passed" })],
      },
      {
        type: "execution",
        turnId: "turn-1",
        sourceExecutionId: "call-b",
        command: expect.objectContaining({ id: "command-b", text: "pnpm test" }),
        results: [expect.objectContaining({ id: "result-b", text: "tests passed" })],
      },
    ]);
  });

  it("renders command and results as children of one Execution unit", () => {
    const displayNodes = buildConversationDisplayNodes(
      [
        projectedCard("command-a", "command", "pnpm typecheck"),
        projectedCard("result-a", "result", "types passed"),
      ],
      [{
        type: "execution",
        turn_id: "turn-1",
        source_execution_id: "call-a",
        command_card_index: 0,
        result_card_indices: [1],
      }],
    );

    const html = renderToStaticMarkup(
      <ConversationContentCards
        blocks={[]}
        nodes={displayNodes}
        t={t}
        visibility={{ command: true, result: true }}
      />,
    );

    expect(html).toContain('data-conversation-execution-id="call-a"');
    expect(html).toContain("执行");
    expect(html).toContain("pnpm typecheck");
    expect(html).toContain("types passed");
    expect(html.indexOf("pnpm typecheck")).toBeLessThan(html.indexOf("types passed"));
  });

  it("does not infer markdown formatting from declared plain command output", () => {
    const blocks = buildConversationContentBlocks([
      commandPart(),
      resultPart([
        "Chunk ID: 089b2c",
        "Wall time: 0.0000 seconds",
        "Process exited with code 0",
        "Original token count: 2116",
        "Output:",
        "---",
        "name: api-and-interface-design",
        "description: Guides stable API and interface design.",
        "---",
        "",
        "# API and Interface Design",
        "",
        "## Overview",
        "",
        "Design stable, well-documented interfaces.",
      ].join("\n")),
    ]);

    expect(blocks.map((block) => block.type)).toEqual(["command", "result"]);
    expect(blocks[1]).toMatchObject({
      format: "plain",
      id: "part-command-result",
      type: "result",
    });

    const html = renderToStaticMarkup(
      <ConversationContentCards
        blocks={blocks}
        resultPreviewLineLimit={30}
        t={t}
        visibility={{
          answer: true,
          code: true,
          command: true,
          result: true,
          tool: true,
        }}
      />,
    );

    expect(html).not.toContain('data-result-format="markdown"');
    expect(html).not.toContain("<h3");
    expect(html).toContain("API and Interface Design");
    expect(html).not.toContain("<h4");
    expect(html).toContain("Overview");
  });

  it("renders diff-language code cards with the unified diff viewer", () => {
    const blocks = buildConversationContentBlocks([], [{
      card_id: "conversation-part-diff",
      part_id: "conversation-part-diff",
      adapter_id: "codex",
      kind: "codex.code",
      semantic_role: "code",
      renderer: "code",
      role: "assistant",
      body: [
        "--- a/src/value.ts",
        "+++ b/src/value.ts",
        "@@ -1 +1 @@",
        "-export const value = 1;",
        "+export const value = 2;",
      ].join("\n"),
      language: "diff",
      legacy_anchor_ids: [],
    }]);

    const html = renderToStaticMarkup(
      <ConversationContentCards
        blocks={blocks}
        t={t}
        visibility={{ code: true }}
      />,
    );

    expect(html).toContain('data-conversation-diff="unified"');
    expect(html).toContain('data-diff-file="src/value.ts"');
    expect(html).toContain('data-diff-line-type="deletion"');
    expect(html).toContain('data-diff-line-type="addition"');
  });


  it("renders terminal output cards whose body is a unified diff with the diff viewer", () => {
    const blocks = buildConversationContentBlocks([], [{
      card_id: "conversation-part-terminal-diff",
      part_id: "conversation-part-terminal-diff",
      adapter_id: "opencode",
      kind: "opencode.result",
      semantic_role: "result",
      renderer: "terminal_output",
      role: "tool",
      body: [
        "--- a/frontend/src/App.tsx",
        "+++ b/frontend/src/App.tsx",
        "@@ -1,2 +1,2 @@",
        "-<OldView />",
        "+<NewView />",
      ].join("\n"),
      legacy_anchor_ids: [],
    }]);

    const html = renderToStaticMarkup(
      <ConversationContentCards
        blocks={blocks}
        t={t}
        visibility={{ result: true }}
      />,
    );

    expect(html).toContain('data-conversation-diff="unified"');
    expect(html).toContain('data-diff-line-type="deletion"');
    expect(html).toContain('data-diff-line-type="addition"');
  });

  it("keeps ordinary plain output and shell transcripts in the plain viewer", () => {
    const blocks = buildConversationContentBlocks([], [{
      card_id: "conversation-part-plain-output",
      part_id: "conversation-part-plain-output",
      adapter_id: "claude-code",
      kind: "claude-code.result",
      semantic_role: "result",
      renderer: "plain",
      role: "tool",
      body: "Claude Code checked 18 files and produced 2 file changes.\nAll checks passed before finishing.",
      legacy_anchor_ids: [],
    }]);

    const html = renderToStaticMarkup(
      <ConversationContentCards
        blocks={blocks}
        t={t}
        visibility={{ result: true }}
      />,
    );

    expect(html).not.toContain('data-conversation-diff="unified"');
    expect(html).toContain("Claude Code checked 18 files");
  });

  it("does not treat git show transcripts with commit headers as unified diffs", () => {
    const blocks = buildConversationContentBlocks([], [{
      card_id: "conversation-part-git-show",
      part_id: "conversation-part-git-show",
      adapter_id: "opencode",
      kind: "opencode.result",
      semantic_role: "result",
      renderer: "plain",
      role: "tool",
      body: [
        "commit 4996af406f992b1777b58e7255338376087554df",
        "Author: Util6",
        "",
        "    fix: refresh mount state",
        "",
        "diff --git a/frontend/src/App.tsx b/frontend/src/App.tsx",
        "index 1234567..7654321 100644",
        "--- a/frontend/src/App.tsx",
        "+++ b/frontend/src/App.tsx",
      ].join("\n"),
      legacy_anchor_ids: [],
    }]);

    const html = renderToStaticMarkup(
      <ConversationContentCards
        blocks={blocks}
        t={t}
        visibility={{ result: true }}
      />,
    );

    expect(html).not.toContain('data-conversation-diff="unified"');
    expect(html).toContain("fix: refresh mount state");
  });
  it("renders plain result cards whose body is a unified diff with the diff viewer", () => {
    const blocks = buildConversationContentBlocks([], [{
      card_id: "conversation-part-git-diff",
      part_id: "conversation-part-git-diff",
      adapter_id: "claude-code",
      kind: "claude-code.result",
      semantic_role: "result",
      renderer: "plain",
      role: "tool",
      body: [
        "diff --git a/cli/cmd/conversation.go b/cli/cmd/conversation.go",
        "--- a/cli/cmd/conversation.go",
        "+++ b/cli/cmd/conversation.go",
        "@@ -12,3 +12,3 @@ func main() {",
        "-  legacy := runLegacy()",
        "+  updated := runUpdated()",
        " }",
      ].join("\n"),
      legacy_anchor_ids: [],
    }]);

    const html = renderToStaticMarkup(
      <ConversationContentCards
        blocks={blocks}
        t={t}
        visibility={{ result: true }}
      />,
    );

    expect(html).toContain('data-conversation-diff="unified"');
    expect(html).toContain('data-diff-file="cli/cmd/conversation.go"');
    expect(html).toContain('data-diff-line-type="deletion"');
    expect(html).toContain('data-diff-line-type="addition"');
    expect(html).not.toContain("data-result-format=");
  });

  it("inserts translated content for a custom target language after opencode is available", async () => {
    const translator = vi.fn().mockResolvedValue({ translated_text: "Ejecuta `pnpm test`." });
    const translationSaver = vi.fn().mockResolvedValue(undefined);

    render(
      <ConversationContentCards
        blocks={[{
          id: "part-answer-answer",
          partId: "part-answer",
          role: "assistant",
          text: "Run `pnpm test`.",
          type: "answer",
        }]}
        t={t}
        translationAvailabilityChecker={async () => ({
          available: true,
          error: null,
          version: "opencode 1.0.0",
        })}
        translationSaver={translationSaver}
        translationSettings={{
          cli: "opencode",
          model: "anthropic/claude-sonnet-4-20250514",
          promptTemplate: "Translate to {targetLanguage}: {content}",
          provider: "cli",
          targetLanguage: "Spanish (Latin America)",
        }}
        translator={translator}
        visibility={{
          answer: true,
          code: true,
          command: true,
          result: true,
          tool: true,
        }}
      />,
    );

    const translateButton = await screen.findByRole("button", { name: "翻译回答文字为Spanish (Latin America)" });
    fireEvent.click(translateButton);

    await waitFor(() =>
      expect(translator).toHaveBeenCalledWith({
        cli: "opencode",
        model: "anthropic/claude-sonnet-4-20250514",
        promptTemplate: "Translate to {targetLanguage}: {content}",
        provider: "cli",
        targetLanguage: "Spanish (Latin America)",
        text: "Run `pnpm test`.",
      }),
    );
    expect(translationSaver).toHaveBeenCalledWith({
      partId: "part-answer",
      recordKind: "session",
      translatedText: "Ejecuta `pnpm test`.",
    });
    expect(await screen.findByText("译文 · Spanish (Latin America)")).toBeTruthy();
    expect(await screen.findByText(/Ejecuta/)).toBeTruthy();
  });

  it("renders saved translated text from the content part", () => {
    const blocks = buildConversationContentBlocks([
      {
        id: "part-answer",
        turn_id: "turn-1",
        part_index: 0,
        role: "assistant",
        kind: "text",
        text: "Run `pnpm test`.",
        translated_text: "运行 `pnpm test`。",
        metadata_json: JSON.stringify({
          content_card: {
            type: "answer",
          },
        }),
      },
    ]);

    render(
      <ConversationContentCards
        blocks={blocks}
        t={t}
        translationAvailabilityChecker={async () => ({
          available: false,
          error: "not found",
          version: null,
        })}
        visibility={{
          answer: true,
          code: true,
          command: true,
          result: true,
          tool: true,
        }}
      />,
    );

    expect(screen.getByText("译文 · 简体中文")).toBeTruthy();
    expect(screen.getByText(/运行/)).toBeTruthy();
  });

  it("enables translation after StrictMode replays the availability effect", async () => {
    const availabilityChecker = vi.fn().mockResolvedValue({
      available: true,
      error: null,
      version: "opencode 1.0.0",
    });

    render(
      <StrictMode>
        <ConversationContentCards
          blocks={[{
            id: "part-answer-answer",
            partId: "part-answer",
            role: "assistant",
            text: "Run `pnpm test`.",
            type: "answer",
          }]}
          t={t}
          translationAvailabilityChecker={availabilityChecker}
          visibility={{
            answer: true,
            code: true,
            command: true,
            result: true,
            tool: true,
          }}
        />
      </StrictMode>,
    );

    const translateButton = await screen.findByRole("button", { name: "翻译回答文字为简体中文" });

    expect((translateButton as HTMLButtonElement).disabled).toBe(false);
  });

  it("disables translation when opencode is unavailable", async () => {
    render(
      <ConversationContentCards
        blocks={[{
          id: "part-answer-answer",
          partId: "part-answer",
          role: "assistant",
          text: "Run tests.",
          type: "answer",
        }]}
        t={t}
        translationAvailabilityChecker={async () => ({
          available: false,
          error: "not found",
          version: null,
        })}
        visibility={{
          answer: true,
          code: true,
          command: true,
          result: true,
          tool: true,
        }}
      />,
    );

    const translateButton = await screen.findByRole("button", {
      name: "翻译服务不可用，无法翻译",
    });
    expect((translateButton as HTMLButtonElement).disabled).toBe(true);
  });
});

const t: Translator = (key, params) => interpolate(messages.zh[key] ?? key, params);

function interpolate(template: string, params?: TranslationParams) {
  if (!params) return template;
  return template.replace(/\{\{(\w+)\}\}/g, (_, key: string) => String(params[key] ?? ""));
}

function commandPart(): ConversationPart {
  return {
    id: "part-command",
    turn_id: "turn-1",
    part_index: 0,
    role: "tool",
    kind: "command",
    command: "sed -n '1,120p' frontend/src/services/conversations.ts",
    status: "completed",
    exit_code: 0,
    metadata_json: JSON.stringify({
      content_card: {
        type: "command",
      },
    }),
  };
}

function resultPart(text: string): ConversationPart {
  return {
    id: "part-command",
    turn_id: "turn-1",
    part_index: 1,
    role: "tool",
    kind: "tool",
    text,
    metadata_json: JSON.stringify({
      content_card: {
        type: "result",
        format: "plain",
        suffix: "result",
      },
    }),
  };
}

function declaredPart(
  id: string,
  type: "answer" | "tool" | "command" | "code" | "result",
  text: string,
): ConversationPart {
  return {
    id,
    turn_id: "turn-1",
    part_index: 0,
    role: type === "answer" || type === "code" ? "assistant" : "tool",
    kind: type === "code" ? "code_block" : "text",
    text,
    metadata_json: JSON.stringify({
      content_card: {
        type,
        format: type === "answer" || type === "code" ? "markdown" : "plain",
      },
    }),
  };
}

function projectedCard(
  id: string,
  semanticRole: "command" | "result",
  body: string,
): ConversationCard {
  return {
    card_id: id,
    part_id: `part-${id}`,
    adapter_id: "codex",
    kind: `codex.${semanticRole}`,
    semantic_role: semanticRole,
    renderer: semanticRole === "command" ? "command" : "terminal_output",
    role: "tool",
    body,
    legacy_anchor_ids: [],
  };
}
