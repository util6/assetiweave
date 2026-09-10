/* @vitest-environment jsdom */

import { fireEvent, render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { I18nProvider } from "../../i18n/I18nProvider";
import type { AgentSessionItemView } from "../../types/agentSession";
import { AgentSessionItem } from "./AgentSessionItem";
import { AgentSessionToolDetail } from "./AgentSessionToolDetail";

function renderWithI18n(ui: React.ReactElement) {
  return render(<I18nProvider>{ui}</I18nProvider>);
}

describe("AgentSessionToolDetail (T05)", () => {
  let writeTextMock: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    writeTextMock = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", {
      value: { writeText: writeTextMock },
      configurable: true,
      writable: true,
    });
  });

  it("renders command block with cwd and copies command", async () => {
    const item: AgentSessionItemView = {
      id: "tool-cmd-1",
      kind: "tool",
      sequence: 1,
      delivery: "live",
      state: "succeeded",
      toolName: "run_command",
      toolInput: {
        command: "cargo test --workspace",
        cwd: "/Users/test/repo",
      },
      turnId: "turn-1",
    };

    renderWithI18n(<AgentSessionToolDetail item={item} />);

    // Command rendered
    expect(screen.getByText("$ cargo test --workspace")).toBeTruthy();
    expect(screen.getByText("cwd: /Users/test/repo")).toBeTruthy();

    // Copy command
    const copyBtn = screen.getByRole("button", { name: "Copy command" });
    fireEvent.click(copyBtn);
    expect(writeTextMock).toHaveBeenCalledWith("cargo test --workspace");
  });

  it("renders terminal stdout and stderr sections with ANSI stripped and exit code", () => {
    const item: AgentSessionItemView = {
      id: "tool-term-1",
      kind: "tool",
      sequence: 2,
      delivery: "live",
      state: "failed",
      toolName: "terminal",
      toolInput: {
        cmd: "npm run build",
      },
      toolOutput: {
        stdout: "\x1b[32mBuild started\x1b[0m\n[1/3] compiling...",
        stderr: "\x1b[31mError: module not found\x1b[0m",
        exit_code: 1,
        signal: "SIGTERM",
      },
      turnId: "turn-1",
    };

    renderWithI18n(<AgentSessionToolDetail item={item} />);

    // ANSI stripped in stdout and stderr
    expect(screen.getByText(/Build started/)).toBeTruthy();
    expect(screen.getByText(/compiling\.\.\./)).toBeTruthy();
    expect(screen.queryByText(/\[32m/)).toBeNull();

    expect(screen.getByText(/Error: module not found/)).toBeTruthy();
    expect(screen.queryByText(/\[31m/)).toBeNull();

    // Exit code and signal
    expect(screen.getByText("Exit code: 1")).toBeTruthy();
    expect(screen.getByText("Signal: SIGTERM")).toBeTruthy();

    // Copy stdout and stderr
    const copyStdoutBtn = screen.getByRole("button", { name: "Copy stdout" });
    fireEvent.click(copyStdoutBtn);
    expect(writeTextMock).toHaveBeenCalledWith(
      "Build started\n[1/3] compiling...",
    );

    const copyStderrBtn = screen.getByRole("button", { name: "Copy stderr" });
    fireEvent.click(copyStderrBtn);
    expect(writeTextMock).toHaveBeenCalledWith("Error: module not found");
  });

  it("displays truncation metadata when present", () => {
    const item: AgentSessionItemView = {
      id: "tool-trunc-1",
      kind: "tool",
      sequence: 3,
      delivery: "live",
      state: "succeeded",
      toolName: "read_file",
      toolOutput: {
        stdout: "head output ... tail output",
      },
      truncation: {
        originalBytes: 12000,
        retainedBytes: 3000,
        strategy: "headTail",
      },
      turnId: "turn-1",
    };

    renderWithI18n(<AgentSessionToolDetail item={item} />);

    expect(
      screen.getByTestId("agent-session-tool-truncation-tool-trunc-1"),
    ).toBeTruthy();
    expect(screen.getByText(/(?:Truncated|已截断)/)).toBeTruthy();
    expect(screen.getByText(/3000.*12000/)).toBeTruthy();
  });

  it("falls back to generic formatted input and output when not a command/terminal", () => {
    const item: AgentSessionItemView = {
      id: "tool-generic-1",
      kind: "tool",
      sequence: 4,
      delivery: "live",
      state: "succeeded",
      toolName: "get_weather",
      toolInput: { city: "Tokyo", units: "metric" },
      toolOutput: { temperature: 22, condition: "Sunny" },
      turnId: "turn-1",
    };

    renderWithI18n(<AgentSessionToolDetail item={item} />);

    expect(
      screen.getByTestId("agent-session-tool-input-tool-generic-1"),
    ).toBeTruthy();
    expect(
      screen.getByTestId("agent-session-tool-output-tool-generic-1"),
    ).toBeTruthy();
    expect(screen.getByText(/"city": "Tokyo"/)).toBeTruthy();
    expect(screen.getByText(/"temperature": 22/)).toBeTruthy();
  });

  it("AgentSessionItem renders AgentSessionToolDetail only when details are open (lazy rendering)", () => {
    const item: AgentSessionItemView = {
      id: "tool-lazy-1",
      kind: "tool",
      sequence: 5,
      delivery: "live",
      state: "succeeded",
      toolName: "run_command",
      toolInput: { command: "echo 'heavy output'" },
      toolOutput: { stdout: "heavy output" },
      turnId: "turn-1",
    };

    renderWithI18n(<AgentSessionItem item={item} />);

    // Initially collapsed: tool detail is NOT rendered in DOM
    expect(
      screen.queryByTestId("agent-session-tool-detail-tool-lazy-1"),
    ).toBeNull();

    // Toggle details to open
    const detailsEl = screen.getByTestId(
      "agent-session-session-item-details-tool-lazy-1",
    ) as HTMLDetailsElement;
    detailsEl.open = true;
    fireEvent(detailsEl, new Event("toggle"));

    // When details are opened, AgentSessionToolDetail is rendered
    expect(
      screen.getByTestId("agent-session-tool-detail-tool-lazy-1"),
    ).toBeTruthy();
    expect(screen.getByText("$ echo 'heavy output'")).toBeTruthy();
  });
});
