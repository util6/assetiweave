// @vitest-environment jsdom

import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { I18nProvider } from "../../i18n/I18nProvider";
import {
  DEFAULT_INTERACTIVE_CAPABILITIES,
  DEFAULT_OBSERVER_CAPABILITIES,
  type AgentSessionItemView,
} from "../../types/agentSession";
import { AgentSessionWorkspace } from "./AgentSessionWorkspace";

describe("AgentSessionWorkspace", () => {
  afterEach(() => cleanup());

  const sampleItems: AgentSessionItemView[] = [
    {
      id: "item-user-1",
      kind: "user_message",
      sequence: 1,
      delivery: "live",
      state: "completed",
      text: "Hello agent",
    },
    {
      id: "item-assistant-1",
      kind: "assistant_text",
      sequence: 2,
      delivery: "live",
      state: "completed",
      text: "I am ready to help.",
    },
    {
      id: "item-processing-1",
      kind: "processing",
      sequence: 3,
      delivery: "live",
      state: "streaming",
      status: "Executing pipeline",
    },
    {
      id: "item-error-1",
      kind: "error",
      sequence: 4,
      delivery: "live",
      state: "failed",
      code: "EXEC_TIMEOUT",
      text: "Action timed out",
    },
  ];

  it("renders assistant, processing, and error items with domain-neutral view models", () => {
    render(
      <I18nProvider>
        <AgentSessionWorkspace
          capabilities={DEFAULT_INTERACTIVE_CAPABILITIES}
          items={sampleItems}
          recipientTitle="Autonomous Agent"
        />
      </I18nProvider>,
    );

    expect(
      screen.getByTestId("agent-session-active-recipient").textContent,
    ).toContain("Autonomous Agent");
    expect(screen.getByText("Hello agent")).toBeTruthy();
    expect(screen.getByText("I am ready to help.")).toBeTruthy();
    expect(screen.getByText("Action timed out")).toBeTruthy();
    expect(screen.getByText("EXEC_TIMEOUT")).toBeTruthy();
  });

  it("shows composer when capabilities.send is true and triggers onSend", () => {
    const onSend = vi.fn();
    render(
      <I18nProvider>
        <AgentSessionWorkspace
          capabilities={{ ...DEFAULT_INTERACTIVE_CAPABILITIES, send: true }}
          draft="Fix this function"
          items={sampleItems}
          onSend={onSend}
          recipientTitle="Coder Agent"
        />
      </I18nProvider>,
    );

    const composer = screen.getByLabelText("Message content");
    expect(composer).toBeTruthy();
    const sendButton = screen.getByRole("button", { name: /send/i });
    expect(sendButton).toBeTruthy();

    fireEvent.click(sendButton);
    expect(onSend).toHaveBeenCalledWith("Fix this function");
  });

  it("hides composer completely when capabilities.send is false (observer mode)", () => {
    render(
      <I18nProvider>
        <AgentSessionWorkspace
          capabilities={{ ...DEFAULT_OBSERVER_CAPABILITIES, send: false }}
          items={sampleItems}
          recipientTitle="Observer Session"
        />
      </I18nProvider>,
    );

    expect(screen.queryByLabelText("Message content")).toBeNull();
    expect(screen.queryByTestId("agent-session-composer")).toBeNull();
  });
});
