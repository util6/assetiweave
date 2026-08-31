// @vitest-environment jsdom

import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { I18nProvider } from "../../i18n/I18nProvider";
import { TeamPage } from "./TeamPage";

const listTeamsMock = vi.hoisted(() => vi.fn());
const createTeamMock = vi.hoisted(() => vi.fn());
const updateTeamMock = vi.hoisted(() => vi.fn());
const deleteTeamMock = vi.hoisted(() => vi.fn());
const listAgentCatalogMock = vi.hoisted(() => vi.fn());
const listAgentMarketMock = vi.hoisted(() => vi.fn());
const listAgentModelsMock = vi.hoisted(() => vi.fn());

vi.mock("../../services/team", () => ({
  createTeam: createTeamMock,
  deleteTeam: deleteTeamMock,
  listTeams: listTeamsMock,
  updateTeam: updateTeamMock,
}));

vi.mock("../../services/agentRuntime", () => ({
  listAgentCatalog: listAgentCatalogMock,
  listAgentMarket: listAgentMarketMock,
  listAgentModels: listAgentModelsMock,
}));

const fixture = {
  id: "team-1",
  name: "Refactor crew",
  description: "Fixed roster",
  created_at: "2026-08-31T00:00:00Z",
  updated_at: "2026-08-31T00:00:00Z",
  members: [
    { id: "leader", team_id: "team-1", role: "leader" as const, sort_order: 0, agent_id: "agent-a", model: "model-a", execution_context_key: "ctx-leader", created_at: "2026-08-31T00:00:00Z", updated_at: "2026-08-31T00:00:00Z" },
    { id: "teammate", team_id: "team-1", role: "teammate" as const, sort_order: 1, agent_id: "agent-b", model: "model-b", execution_context_key: "ctx-teammate", created_at: "2026-08-31T00:00:00Z", updated_at: "2026-08-31T00:00:00Z" },
  ],
};

function renderPage() {
  return render(<I18nProvider><TeamPage /></I18nProvider>);
}

describe("TeamPage", () => {
  beforeEach(() => {
    listTeamsMock.mockResolvedValue([fixture]);
    createTeamMock.mockResolvedValue({ ...fixture, id: "team-2", name: "New crew" });
    updateTeamMock.mockResolvedValue(fixture);
    deleteTeamMock.mockResolvedValue(undefined);
    listAgentCatalogMock.mockResolvedValue([
      { id: "agent-a", display_name: "Agent A", command: "agent-a", args: [], availability_command: "agent-a", protocol: "acp" },
      { id: "agent-b", display_name: "Agent B", command: "agent-b", args: [], availability_command: "agent-b", protocol: "native" },
    ]);
    listAgentMarketMock.mockResolvedValue([]);
    listAgentModelsMock.mockImplementation(async (agentId: string) => ({ agent_id: agentId, available: true, models: [{ id: agentId === "agent-a" ? "model-a" : "model-b", label: agentId === "agent-a" ? "Model A" : "Model B", description: null }], current_model_id: agentId === "agent-a" ? "model-a" : "model-b", error_code: null, error: null }));
  });

  afterEach(() => cleanup());

  it("loads a roster and moves a member before saving", async () => {
    renderPage();
    await waitFor(() => expect(screen.getAllByText("Refactor crew").length).toBeGreaterThan(0));
    fireEvent.click(screen.getByRole("button", { name: "Edit" }));
    await waitFor(() => expect(screen.getByRole("dialog")).toBeTruthy());

    const upButtons = screen.getAllByRole("button", { name: "Move up" });
    fireEvent.click(upButtons[1]);
    fireEvent.click(screen.getByRole("button", { name: "Save team" }));

    await waitFor(() => expect(updateTeamMock).toHaveBeenCalledWith(expect.objectContaining({
      team_id: "team-1",
      members: expect.arrayContaining([
        expect.objectContaining({ id: "teammate", sort_order: 0 }),
        expect.objectContaining({ id: "leader", sort_order: 1 }),
      ]),
    })));
  });

  it("creates through the service with catalog-selected members", async () => {
    renderPage();
    await waitFor(() => expect(screen.getAllByText("Refactor crew").length).toBeGreaterThan(0));
    fireEvent.click(screen.getByRole("button", { name: "Create team" }));
    fireEvent.change(screen.getByLabelText("Team name"), { target: { value: "New crew" } });
    fireEvent.click(screen.getByRole("button", { name: "Save team" }));
    await waitFor(() => expect(createTeamMock).toHaveBeenCalledWith(expect.objectContaining({ name: "New crew" })));
  });

  it("uses the shared confirm dialog for deletion", async () => {
    renderPage();
    await waitFor(() => expect(screen.getAllByText("Refactor crew").length).toBeGreaterThan(0));
    fireEvent.click(screen.getByRole("button", { name: "Delete" }));
    expect(screen.getByRole("dialog")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Delete" }));
    await waitFor(() => expect(deleteTeamMock).toHaveBeenCalledWith("team-1"));
  });
});
