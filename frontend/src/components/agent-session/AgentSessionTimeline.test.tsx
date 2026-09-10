/* @vitest-environment jsdom */

import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { I18nProvider } from "../../i18n/I18nProvider";
import type { AgentSessionItemView } from "../../types/agentSession";
import { AgentSessionTimeline } from "./AgentSessionTimeline";

function renderWithI18n(ui: React.ReactElement) {
  return render(<I18nProvider>{ui}</I18nProvider>);
}

describe("AgentSessionTimeline with Turn and StepGroup (T03)", () => {
  const canonicalItems: AgentSessionItemView[] = [
    {
      id: "user-1",
      kind: "user_message",
      sequence: 1,
      delivery: "live",
      state: "completed",
      text: "Inspect the workspace and update the target.",
      turnId: "turn-1",
    },
    {
      id: "asst-1",
      kind: "assistant_text",
      sequence: 2,
      delivery: "live",
      state: "streaming",
      text: "I will inspect",
      turnId: "turn-1",
    },
    {
      id: "think-1",
      kind: "thinking",
      sequence: 3,
      delivery: "live",
      state: "streaming",
      text: "Analyzing the codebase...",
      turnId: "turn-1",
    },
    {
      id: "tool-run",
      kind: "tool",
      sequence: 4,
      delivery: "replay",
      state: "succeeded",
      toolName: "run_command",
      toolCallId: "call-1",
      toolInput: { cmd: "ls" },
      toolOutput: { stdout: "Cargo.toml\n" },
      turnId: "turn-1",
    },
    {
      id: "asst-2",
      kind: "assistant_text",
      sequence: 7,
      delivery: "live",
      state: "completed",
      text: "I found the file.",
      turnId: "turn-1",
    },
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
    {
      id: "asst-3",
      kind: "assistant_text",
      sequence: 11,
      delivery: "live",
      state: "completed",
      text: "All done.",
      turnId: "turn-1",
    },
    {
      id: "term-1",
      kind: "final_result",
      sequence: 12,
      delivery: "live",
      state: "completed",
      turnId: "turn-1",
    },
  ];

  it("renders canonical sequence with StepGroups and preserves user manual toggle", () => {
    renderWithI18n(<AgentSessionTimeline items={canonicalItems} />);

    // 1. User request is displayed
    expect(
      screen.getByText("Inspect the workspace and update the target."),
    ).toBeTruthy();

    // 2. Assistant text is displayed
    expect(screen.getByText("I will inspect")).toBeTruthy();
    expect(screen.getByText("I found the file.")).toBeTruthy();
    expect(screen.getByText("All done.")).toBeTruthy();

    // 3. Thinking item is rendered
    expect(screen.getByTestId("agent-session-thinking-think-1")).toBeTruthy();

    // 4. StepGroup 1: 1 tool (run_command) - succeeded, not failed, so default collapsed
    expect(screen.getByText(/(?:View Steps|查看步骤) · 1/)).toBeTruthy();

    // 5. StepGroup 2: 2 tools (file_edit, cargo_test) - failed, so default expanded
    expect(screen.getByText(/(?:View Steps|查看步骤) · 2/)).toBeTruthy();
    // Failed group is default expanded, so cargo_test is visible
    expect(
      screen.getByTestId("agent-session-session-item-tool-test"),
    ).toBeTruthy();

    // 6. User toggles StepGroup 1 (expand it)
    const toggleGroup1 = screen.getByTestId(
      "agent-session-step-group-toggle-group-turn-1-0",
    );
    fireEvent.click(toggleGroup1);
    expect(
      screen.getByTestId("agent-session-session-item-tool-run"),
    ).toBeTruthy();

    // Toggle again to collapse
    fireEvent.click(toggleGroup1);
    expect(
      screen.queryByTestId("agent-session-session-item-tool-run"),
    ).toBeNull();
  });

  it("renders truncation indicator when item has truncation metadata", () => {
    const itemWithTruncation: AgentSessionItemView[] = [
      {
        id: "asst-trunc",
        kind: "assistant_text",
        sequence: 1,
        delivery: "live",
        state: "completed",
        text: "Part of text\n... [truncated] ...\nEnd of text",
        truncation: {
          originalBytes: 1000,
          retainedBytes: 250,
          strategy: "headTail",
        },
        turnId: "turn-1",
      },
    ];

    renderWithI18n(<AgentSessionTimeline items={itemWithTruncation} />);
    expect(
      screen.getByTestId(
        "agent-session-assistant-text-truncation-asst-trunc",
      ),
    ).toBeTruthy();
    expect(screen.getByText(/(?:Truncated|已截断)/)).toBeTruthy();
  });
});
