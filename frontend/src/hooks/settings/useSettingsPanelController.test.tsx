/* @vitest-environment jsdom */

import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";

import { useSettingsPanelController, type SettingsPanelGroup } from "./useSettingsPanelController";
import type { SettingsPanelId } from "../../store/settings/settingsSchema";

afterEach(cleanup);

const groups: SettingsPanelGroup[] = [
  {
    id: "general",
    panels: [{ id: "general.appearance" }, { id: "general.memory" }],
  },
  {
    id: "agents",
    panels: [{ id: "agents.market" }, { id: "agents.settings" }],
  },
];

function Fixture({ initialPanel = "general.appearance", open = true }: { initialPanel?: SettingsPanelId; open?: boolean }) {
  const controller = useSettingsPanelController({
    groups,
    initialPanel,
    normalizePanel: (panel) => (panel === "general.agents" ? "agents.market" : panel),
    open,
  });

  return (
    <>
      <output data-testid="active-panel">{controller.activePanel}</output>
      <output data-testid="general-state">{controller.collapsedGroups.has("general") ? "collapsed" : "expanded"}</output>
      <output data-testid="agents-state">{controller.collapsedGroups.has("agents") ? "collapsed" : "expanded"}</output>
      <button onClick={() => controller.toggleGroupCollapsed("general")} type="button">
        Toggle general
      </button>
      <button onClick={() => controller.openPanel("general.memory")} type="button">
        Open memory
      </button>
    </>
  );
}

describe("useSettingsPanelController", () => {
  it("normalizes the initial panel and keeps its group expanded", () => {
    render(<Fixture initialPanel="general.agents" />);

    expect(screen.getByTestId("active-panel").textContent).toBe("agents.market");
    expect(screen.getByTestId("agents-state").textContent).toBe("expanded");
  });

  it("owns panel navigation and re-expands a collapsed target group", () => {
    render(<Fixture />);

    fireEvent.click(screen.getByRole("button", { name: "Toggle general" }));
    expect(screen.getByTestId("general-state").textContent).toBe("collapsed");

    fireEvent.click(screen.getByRole("button", { name: "Open memory" }));

    expect(screen.getByTestId("active-panel").textContent).toBe("general.memory");
    expect(screen.getByTestId("general-state").textContent).toBe("expanded");
  });
});
