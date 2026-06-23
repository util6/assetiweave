/* @vitest-environment jsdom */

import { renderToStaticMarkup } from "react-dom/server";
import { describe, expect, it } from "vitest";
import {
  ConversationContentCards,
  buildConversationContentBlocks,
} from "./ConversationContentCards";
import type { Translator } from "../../i18n/I18nProvider";
import { messages, type TranslationParams } from "../../i18n/messages";
import type { ConversationPart } from "../../types";

describe("ConversationContentCards", () => {
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
