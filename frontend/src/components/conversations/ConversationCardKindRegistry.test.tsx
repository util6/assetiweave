/* @vitest-environment jsdom */

import { act, render, screen } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import {
  ConversationCardKindIcon,
  ConversationCardKindRegistryProvider,
  conversationCardPresentationKind,
  isRedundantConversationCardKind,
  useConversationCardKindRegistry,
} from "./ConversationCardKindRegistry";

const listAdapters = vi.hoisted(() => vi.fn());
vi.mock("../../services/conversations", () => ({ listConversationAdapters: listAdapters }));

afterEach(() => {
  vi.clearAllMocks();
});

it("registers Manifest labels and keeps icon hints inside the controlled icon set", async () => {
  listAdapters.mockResolvedValue([{
    card_kinds: [{
      id: "claude-code.reasoning",
      semantic_role: "reasoning",
      label: "Reasoning trace",
      default_renderer: "markdown",
      allowed_renderers: ["markdown"],
      icon_hint: "brain",
    }],
  }]);

  function Probe() {
    const { definitions } = useConversationCardKindRegistry();
    const definition = definitions.get("claude-code.reasoning");
    return (
      <div>
        <span>{definition?.label ?? "missing"}</span>
        <ConversationCardKindIcon iconHint={definition?.icon_hint} renderer="markdown" />
      </div>
    );
  }

  render(<ConversationCardKindRegistryProvider><Probe /></ConversationCardKindRegistryProvider>);
  await act(async () => undefined);

  expect(screen.getByText("Reasoning trace")).toBeTruthy();
  expect(document.querySelector("svg.lucide-brain")).toBeTruthy();
});

it("falls back to the renderer icon for an unknown Manifest icon hint", () => {
  render(<ConversationCardKindIcon iconHint="adapter-owned-svg" renderer="json" />);
  expect(document.querySelector("svg.lucide-braces")).toBeTruthy();
});

it("uses the Core built-in icon before the renderer fallback", () => {
  render(<ConversationCardKindIcon kind="tool" renderer="plain" />);
  expect(document.querySelector("svg.lucide-wrench")).toBeTruthy();
});

it("keeps one presentation type for namespaced built-in semantics", () => {
  const definition = {
    id: "claude-code.command",
    semantic_role: "command",
    label: "Command",
    default_renderer: "command" as const,
    allowed_renderers: ["command" as const],
  };

  expect(conversationCardPresentationKind(definition.id, definition.semantic_role)).toBe("command");
  expect(isRedundantConversationCardKind(definition.id, definition)).toBe(true);
  expect(isRedundantConversationCardKind("claude-code.reasoning", {
    ...definition,
    id: "claude-code.reasoning",
    semantic_role: "reasoning",
  })).toBe(false);
});

it("refreshes after Adapter changes without retaining an uninstalled Manifest definition", async () => {
  listAdapters
    .mockResolvedValueOnce([{
      card_kinds: [{
        id: "custom.trace",
        semantic_role: "reasoning",
        label: "Custom trace",
        default_renderer: "json",
        allowed_renderers: ["json"],
      }],
    }])
    .mockResolvedValueOnce([]);

  function Probe() {
    const { definitions } = useConversationCardKindRegistry();
    return <span>{definitions.get("custom.trace")?.label ?? "unregistered"}</span>;
  }

  render(<ConversationCardKindRegistryProvider><Probe /></ConversationCardKindRegistryProvider>);
  expect(await screen.findByText("Custom trace")).toBeTruthy();
  window.dispatchEvent(new Event("assetiweave:conversation-adapters-changed"));
  expect(await screen.findByText("unregistered")).toBeTruthy();
});
