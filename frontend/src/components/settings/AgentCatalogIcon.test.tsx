/* @vitest-environment jsdom */

import { cleanup, render } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { AgentCatalogIcon } from "./AgentCatalogIcon";
import { agentCatalog } from "./agentCatalog";

afterEach(() => {
  cleanup();
});

describe("AgentCatalogIcon", () => {
  it.each([
    ["opencode", "text-primary"],
    ["gemini", "text-status-update"],
    ["kiro", "text-status-create"],
    ["hermes", "text-on-surface-variant"],
    ["pi", "text-status-update"],
  ] as const)("uses the semantic theme color for %s", (agentId, colorClass) => {
    const agent = agentCatalog.find((item) => item.id === agentId);
    expect(agent).toBeTruthy();

    const { container } = render(<AgentCatalogIcon agent={agent!} />);
    expect(container.querySelector("svg")?.classList.contains(colorClass)).toBe(true);
  });
});
