/* @vitest-environment jsdom */

import { cleanup, render } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { AgentCatalogIcon, resolveAgentIconAccentColor } from "./AgentCatalogIcon";
import { agentCatalog } from "./agentCatalog";

afterEach(() => {
  cleanup();
});

describe("AgentCatalogIcon", () => {
  it("uses the configured APP accent color instead of an Agent-specific tone", () => {
    const agent = agentCatalog.find((item) => item.id === "hermes");
    expect(agent).toBeTruthy();

    const configuredShortcut = {
      profileId: "hermes",
      profileName: "Hermes",
      appKind: "custom" as const,
      displayIcon: "app:hermes",
      accentColor: "#123456",
      enabled: true,
    };

    expect(resolveAgentIconAccentColor(agent!, [configuredShortcut])).toBe("#123456");

    const { container } = render(<AgentCatalogIcon agent={agent!} appShortcuts={[configuredShortcut]} />);
    expect(container.querySelector("svg")?.getAttribute("style")).toContain("color: rgb(18, 52, 86)");
  });

  it("falls back to the shared built-in APP color when no shortcut override exists", () => {
    const agent = agentCatalog.find((item) => item.id === "hermes");
    expect(agent).toBeTruthy();

    expect(resolveAgentIconAccentColor(agent!, [])).toBeTruthy();
    const { container } = render(<AgentCatalogIcon agent={agent!} />);
    expect(container.querySelector("svg")?.getAttribute("style")).toContain("color:");
  });

  it("uses the shared fallback class for agents without a registered APP icon", () => {
    const agent = agentCatalog.find((item) => item.id === "pi");
    expect(agent).toBeTruthy();

    const { container } = render(<AgentCatalogIcon agent={agent!} />);
    expect(container.querySelector("svg")?.classList.contains("text-primary")).toBe(true);
  });
});
