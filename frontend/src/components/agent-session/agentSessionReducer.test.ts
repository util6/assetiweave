import { describe, expect, it } from "vitest";
import type { AgentSessionItemView } from "../../types/agentSession";
import {
  buildAgentSessionTurns,
  calculateStepGroupStatus,
  isGroupDefaultExpanded,
} from "./agentSessionReducer";

describe("agentSessionReducer", () => {
  it("converts canonical complete fixture items into ordered turns and step groups", () => {
    const canonicalItems: AgentSessionItemView[] = [
      // 1. User request
      {
        id: "user-1",
        kind: "user_message",
        sequence: 1,
        delivery: "live",
        state: "completed",
        text: "Inspect the workspace and update the target.",
        turnId: "turn-1",
      },
      // 2. Assistant text delta
      {
        id: "asst-1",
        kind: "assistant_text",
        sequence: 2,
        delivery: "live",
        state: "streaming",
        text: "I will inspect",
        turnId: "turn-1",
      },
      // 3. Thinking
      {
        id: "think-1",
        kind: "thinking",
        sequence: 3,
        delivery: "live",
        state: "streaming",
        text: "Analyzing the codebase...",
        turnId: "turn-1",
      },
      // 4-6. Tool A (single logical tool materialized)
      {
        id: "tool-run",
        kind: "tool",
        sequence: 4,
        delivery: "live",
        state: "succeeded",
        toolName: "run_command",
        toolCallId: "call-1",
        toolInput: { cmd: "ls" },
        toolOutput: { stdout: "Cargo.toml\n" },
        turnId: "turn-1",
      },
      // 7. Assistant text
      {
        id: "asst-2",
        kind: "assistant_text",
        sequence: 7,
        delivery: "live",
        state: "completed",
        text: "I found the file.",
        turnId: "turn-1",
      },
      // 8-9. Tool B (file edit)
      {
        id: "tool-edit",
        kind: "tool",
        sequence: 8,
        delivery: "live",
        state: "succeeded",
        toolName: "file_edit",
        toolCallId: "call-2",
        toolInput: { file: "Cargo.toml" },
        toolOutput: { diff: "+[dependencies]" },
        turnId: "turn-1",
      },
      // 10. Tool C (failure)
      {
        id: "tool-test",
        kind: "tool",
        sequence: 10,
        delivery: "live",
        state: "failed",
        toolName: "cargo_test",
        toolCallId: "call-3",
        toolOutput: { stderr: "failed" },
        turnId: "turn-1",
      },
      // 11. Assistant final text
      {
        id: "asst-3",
        kind: "assistant_text",
        sequence: 11,
        delivery: "live",
        state: "completed",
        text: "All done.",
        turnId: "turn-1",
      },
      // 12. Terminal result
      {
        id: "term-1",
        kind: "final_result",
        sequence: 12,
        delivery: "live",
        state: "completed",
        turnId: "turn-1",
      },
    ];

    const turns = buildAgentSessionTurns(canonicalItems);
    expect(turns).toHaveLength(1);
    const turn = turns[0];
    expect(turn.turnId).toBe("turn-1");

    // Blocks:
    // 0: user_request
    // 1: assistant_text
    // 2: thinking
    // 3: step_group (1 tool: run_command)
    // 4: assistant_text
    // 5: step_group (2 tools: file_edit, cargo_test)
    // 6: assistant_text
    // 7: terminal
    expect(turn.blocks).toHaveLength(8);

    expect(turn.blocks[0].type).toBe("user_request");
    expect(turn.blocks[1].type).toBe("assistant_text");
    expect(turn.blocks[2].type).toBe("thinking");

    // Group 1: 1 logical tool
    expect(turn.blocks[3].type).toBe("step_group");
    if (turn.blocks[3].type === "step_group") {
      expect(turn.blocks[3].logicalCount).toBe(1);
      expect(turn.blocks[3].status).toBe("succeeded");
      expect(turn.blocks[3].items[0].toolName).toBe("run_command");
    }

    expect(turn.blocks[4].type).toBe("assistant_text");

    // Group 2: 2 contiguous tools separated by assistant_text
    expect(turn.blocks[5].type).toBe("step_group");
    if (turn.blocks[5].type === "step_group") {
      expect(turn.blocks[5].logicalCount).toBe(2);
      // Because tool-test failed, the overall group status must be failed
      expect(turn.blocks[5].status).toBe("failed");
      expect(turn.blocks[5].items.map((i) => i.toolName)).toEqual([
        "file_edit",
        "cargo_test",
      ]);
    }

    expect(turn.blocks[6].type).toBe("assistant_text");
    expect(turn.blocks[7].type).toBe("terminal");
  });

  it("evaluates default expansion rules based on status and delivery", () => {
    // Failed is always expanded
    expect(isGroupDefaultExpanded("failed", "live")).toBe(true);
    expect(isGroupDefaultExpanded("failed", "replay")).toBe(true);

    // Live running/pending is expanded
    expect(isGroupDefaultExpanded("running", "live")).toBe(true);
    expect(isGroupDefaultExpanded("pending", "live")).toBe(true);

    // Completed replay is collapsed
    expect(isGroupDefaultExpanded("succeeded", "replay")).toBe(false);

    // Live group defaults to true
    expect(isGroupDefaultExpanded("succeeded", "live")).toBe(true);
  });

  it("calculates step group status priority: failed > running > cancelled > succeeded > pending", () => {
    const makeItem = (
      state: AgentSessionItemView["state"],
    ): AgentSessionItemView => ({
      id: "item",
      kind: "tool",
      sequence: 1,
      delivery: "live",
      state,
    });

    expect(
      calculateStepGroupStatus([makeItem("succeeded"), makeItem("failed")]),
    ).toBe("failed");
    expect(
      calculateStepGroupStatus([makeItem("succeeded"), makeItem("streaming")]),
    ).toBe("running");
    expect(
      calculateStepGroupStatus([makeItem("succeeded"), makeItem("cancelled")]),
    ).toBe("cancelled");
    expect(
      calculateStepGroupStatus([makeItem("succeeded"), makeItem("completed")]),
    ).toBe("succeeded");
    expect(calculateStepGroupStatus([makeItem("pending")])).toBe("running");
  });
});
