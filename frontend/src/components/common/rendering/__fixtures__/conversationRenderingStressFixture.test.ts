import { describe, expect, it } from "vitest";
import { createConversationRenderingStressFixture } from "./conversationRenderingStressFixture";

describe("createConversationRenderingStressFixture", () => {
  it("creates 80 stable turns with the required complex content mix", () => {
    const fixture = createConversationRenderingStressFixture();
    const nodes = fixture.projected_content_nodes;

    expect(fixture.turns).toHaveLength(80);
    expect(fixture.turns.map((turn) => turn.id)).toEqual(
      Array.from({ length: 80 }, (_, index) => `stress-turn-${String(index + 1).padStart(2, "0")}`),
    );
    expect(new Set(fixture.turns.map((turn) => turn.id)).size).toBe(80);
    expect(nodes.filter((node) => node.renderer === "code")).toHaveLength(24);
    expect(nodes.filter((node) => node.renderer === "command")).toHaveLength(24);
    expect(nodes.filter((node) => node.node_type === "fixture.result")).toHaveLength(32);
    expect(nodes.filter((node) => node.renderer === "diff")).toHaveLength(12);
    expect(nodes.filter((node) => node.content.includes("| 指标 | 值 |"))).toHaveLength(8);
    expect(nodes.filter((node) => node.content.includes("```mermaid"))).toHaveLength(4);
    expect(nodes.filter((node) => node.content.includes("$$"))).toHaveLength(8);
    expect(nodes.filter((node) => node.node_type === "fixture.answer")).toHaveLength(80);
  });

  it("is deterministic and keeps every card connected to a turn node", () => {
    const first = createConversationRenderingStressFixture();
    const second = createConversationRenderingStressFixture();

    expect(second).toEqual(first);
    expect(first.projected_content_nodes.every((node) => (
      first.turns.some((turn) => turn.id === node.turn_id)
    ))).toBe(true);
  });
});
