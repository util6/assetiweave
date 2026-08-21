/* @vitest-environment jsdom */

import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { I18nProvider } from "../../i18n/I18nProvider";
import { AgentSettingsPanel } from "./AgentSettingsPanel";
import type { AppShortcut } from "../../types";

const listAgentMarketMock = vi.hoisted(() => vi.fn());
const agentRuntime = vi.hoisted(() => ({
  listAgentMarket: undefined as undefined | typeof listAgentMarketMock,
  listAgentCatalog: vi.fn(),
  listAgentModels: vi.fn(),
  checkAgentConnection: vi.fn(),
}));

vi.mock("../../services/agentRuntime", () => agentRuntime);

beforeEach(() => {
  vi.stubGlobal("localStorage", {
    getItem: () => "zh",
    setItem: vi.fn(),
  });
  vi.stubGlobal("navigator", { language: "zh-CN" });
  vi.clearAllMocks();
  agentRuntime.listAgentMarket = undefined;
  agentRuntime.listAgentCatalog.mockResolvedValue([
    "opencode",
    "gemini",
    "kiro",
    "antigravity",
    "claude",
    "codex",
    "hermes",
    "pi",
    "qoder",
  ].map((id) => ({
    id,
    display_name: id,
    command: id,
    args: [],
    availability_command: id,
    protocol: "acp",
  })));
  agentRuntime.checkAgentConnection.mockImplementation((agentId: string) => Promise.resolve({
    agent_id: agentId,
    available: agentId === "opencode" || agentId === "gemini",
    installed: agentId === "opencode" || agentId === "gemini",
    connected: false,
    version: agentId === "opencode" ? "opencode 1.0.0" : agentId === "gemini" ? "gemini 1.0.0" : null,
    connection_method: agentId === "opencode" || agentId === "gemini" ? "acp" : null,
    error_code: agentId === "opencode" || agentId === "gemini" ? null : "command_not_found",
    error: agentId === "opencode" || agentId === "gemini" ? null : `${agentId} was not found`,
  }));
  agentRuntime.listAgentModels.mockResolvedValue({
    agent_id: "codex",
    available: true,
    models: [
      { id: "fixture/model-fast", label: "Fixture Fast", description: "Fast fixture model" },
      { id: "fixture/model-accurate", label: "Fixture Accurate", description: null },
    ],
    current_model_id: "fixture/model-fast",
    error_code: null,
    error: null,
  });
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

describe("AgentSettingsPanel", () => {
  it("loads the builtin availability and renders filterable agent rows", async () => {
    renderPanel();

    expect(screen.getByRole("heading", { name: "Agents" })).toBeTruthy();
    expect(screen.getByRole("heading", { name: "OpenCode" })).toBeTruthy();
    expect(screen.getByRole("heading", { name: "Gemini CLI" })).toBeTruthy();
    const openCodeRow = screen.getByRole("heading", { name: "OpenCode" }).closest("article");
    const geminiRow = (await screen.findByRole("heading", { name: "Gemini CLI" })).closest("article");
    expect(openCodeRow?.querySelector('path[d="M16 6H8v12h8V6zm4 16H4V2h16v20z"]')).toBeTruthy();
    expect(geminiRow?.querySelector('path[d^="M20.616 10.835"]')).toBeTruthy();
    expect(screen.getAllByText("可用").length).toBeGreaterThan(0);
    await waitFor(() => expect(agentRuntime.checkAgentConnection).toHaveBeenCalledWith("opencode", "installation"));

    fireEvent.click(screen.getByRole("tab", { name: /^可用/ }));
    expect(screen.getByRole("heading", { name: "OpenCode" })).toBeTruthy();
    expect(screen.queryByRole("heading", { name: "Codex CLI" })).toBeNull();

    fireEvent.change(screen.getByRole("searchbox", { name: "搜索 Agent" }), {
      target: { value: "claude" },
    });
    expect(screen.queryByRole("heading", { name: "OpenCode" })).toBeNull();
  });

  it("tests a registered Agent and keeps editing inside the Agents page", async () => {
    renderSettingsPanel();

    const geminiRow = (await screen.findByRole("heading", { name: "Gemini CLI" })).closest("article");
    expect(geminiRow).toBeTruthy();
    fireEvent.click(within(geminiRow as HTMLElement).getByRole("button", { name: "测试连接" }));

    await waitFor(() => expect(agentRuntime.checkAgentConnection).toHaveBeenCalledWith("gemini", "connection"));
    await waitFor(() => expect(within(geminiRow as HTMLElement).getByTitle("gemini 1.0.0")).toBeTruthy());

    fireEvent.click(within(geminiRow as HTMLElement).getByRole("button", { name: "编辑 Gemini CLI" }));
    expect(screen.getByRole("heading", { name: "Gemini CLI" })).toBeTruthy();
    expect(screen.getByText(/当前版本在 Agents 页面统一展示命令、协议和连接状态；详细定义编辑将在后续接入/)).toBeTruthy();
  });

  it("passes the global APP accent color to the matching Agent icon", () => {
    const appShortcuts: AppShortcut[] = [{
      profileId: "hermes",
      profileName: "Hermes",
      appKind: "custom",
      displayIcon: "app:hermes",
      accentColor: "#123456",
      enabled: true,
    }];

    renderPanel({ appShortcuts });

    const hermesRow = screen.getByRole("heading", { name: "Hermes" }).closest("article");
    expect(hermesRow?.querySelector("svg")?.getAttribute("style")).toContain("color: rgb(18, 52, 86)");
  });

  it("runs a real ACP connection check for a registered Agent", async () => {
    agentRuntime.checkAgentConnection.mockImplementation((agentId: string, mode: string) => Promise.resolve({
      agent_id: agentId,
      available: agentId === "codex",
      installed: agentId === "codex",
      connected: mode === "connection" && agentId === "codex",
      version: agentId === "codex" ? "codex-acp 1.1.2" : null,
      connection_method: agentId === "codex" ? "acp" : null,
      error_code: agentId === "codex" ? null : "command_not_found",
      error: agentId === "codex" ? null : `${agentId} was not found`,
    }));
    renderSettingsPanel();

    const codexRow = (await screen.findByRole("heading", { name: "Codex CLI" })).closest("article");
    fireEvent.click(within(codexRow as HTMLElement).getByRole("button", { name: "测试连接" }));

    await waitFor(() => expect(agentRuntime.checkAgentConnection).toHaveBeenCalledWith("codex", "connection"));
    await waitFor(() => expect(within(codexRow as HTMLElement).getByTitle("codex-acp 1.1.2")).toBeTruthy());
  });

  it("loads ACP models in a dialog and persists the selected model callback", async () => {
    const onModelChange = vi.fn();
    renderSettingsPanel({ onModelChange });

    const codexRow = (await screen.findByRole("heading", { name: "Codex CLI" })).closest("article");
    fireEvent.click(within(codexRow as HTMLElement).getByRole("button", { name: "模型 Codex CLI" }));

    expect(await screen.findByRole("heading", { name: "选择模型 · Codex CLI" })).toBeTruthy();
    await waitFor(() => expect(agentRuntime.listAgentModels).toHaveBeenCalledWith("codex"));
    const currentModelSection = screen.getByRole("region", { name: "当前模型" });
    const availableModelSection = screen.getByRole("radiogroup", { name: "其他可选模型" });
    expect(within(currentModelSection).getByRole("radio", { name: /Fixture Fast/ })).toBeTruthy();
    expect(within(availableModelSection).queryByRole("radio", { name: /Fixture Fast/ })).toBeNull();
    fireEvent.click(screen.getByRole("radio", { name: /Fixture Accurate/ }));

    expect(onModelChange).toHaveBeenCalledWith("codex", "fixture/model-accurate");
    expect(within(currentModelSection).getByRole("radio", { name: /Fixture Accurate/ })).toBeTruthy();
  });

  it("does not expose a custom Agent definition entry", () => {
    renderPanel();

    expect(screen.queryByRole("button", { name: /添加自定义 Agent/ })).toBeNull();
  });

  it("keeps lifecycle and definition actions in the ACP market view", async () => {
    agentRuntime.listAgentMarket = listAgentMarketMock;
    listAgentMarketMock.mockResolvedValue([createMarketItem("opencode", true)]);

    renderPanel();

    const openCodeRow = await screen.findByRole("heading", { name: "OpenCode" });
    const row = openCodeRow.closest("article");
    expect(within(row as HTMLElement).getByRole("button", { name: "停用" })).toBeTruthy();
    expect(within(row as HTMLElement).getByRole("button", { name: "重装" })).toBeTruthy();
    expect(within(row as HTMLElement).queryByRole("button", { name: "测试连接" })).toBeNull();
    expect(within(row as HTMLElement).queryByRole("button", { name: "模型 OpenCode" })).toBeNull();
    expect(within(row as HTMLElement).queryByRole("button", { name: "编辑 OpenCode" })).toBeNull();
    expect(within(row as HTMLElement).getAllByRole("button")).toHaveLength(2);
  });

  it("treats core compatibility metadata as observational", async () => {
    agentRuntime.listAgentMarket = listAgentMarketMock;
    const item = createMarketItem("opencode", false);
    item.coreCompatible = false;
    listAgentMarketMock.mockResolvedValue([item]);

    renderPanel();

    const row = (await screen.findByRole("heading", { name: "OpenCode" })).closest("article");
    const install = within(row as HTMLElement).getByRole("button", { name: "安装" });
    expect((install as HTMLButtonElement).disabled).toBe(false);
    expect(within(row as HTMLElement).queryByText("当前版本不兼容")).toBeNull();
  });

  it("shows only installed Agents and the three compact ACP settings actions", async () => {
    agentRuntime.listAgentMarket = listAgentMarketMock;
    listAgentMarketMock.mockResolvedValue([
      createMarketItem("opencode", true),
      createMarketItem("codex", false),
    ]);

    renderPanel({ view: "settings" });

    const openCodeRow = await screen.findByRole("heading", { name: "OpenCode" });
    expect(screen.queryByRole("heading", { name: "Codex CLI" })).toBeNull();
    const row = openCodeRow.closest("article");
    expect(row).toBeTruthy();
    expect(within(row as HTMLElement).getAllByRole("button")).toHaveLength(3);
    expect(within(row as HTMLElement).getByRole("button", { name: "测试连接" })).toBeTruthy();
    expect(within(row as HTMLElement).getByRole("button", { name: "模型 OpenCode" })).toBeTruthy();
    expect(within(row as HTMLElement).getByRole("button", { name: "编辑 OpenCode" })).toBeTruthy();
    expect(within(row as HTMLElement).queryByRole("button", { name: /安装状态|更新|重装|卸载/ })).toBeNull();
  });
});

function renderSettingsPanel(options: Parameters<typeof renderPanel>[0] = {}) {
  agentRuntime.listAgentMarket = listAgentMarketMock;
  listAgentMarketMock.mockResolvedValue([
    createMarketItem("opencode", true),
    createMarketItem("gemini", true),
    createMarketItem("codex", true),
  ]);
  return renderPanel({ ...options, view: "settings" });
}

function renderPanel({
  appShortcuts = [],
  onModelChange = vi.fn(),
  view = "market",
}: {
  appShortcuts?: AppShortcut[];
  onModelChange?: (agentId: string, modelId: string) => void;
  view?: "market" | "settings";
} = {}) {
  return render(
    <I18nProvider>
      <AgentSettingsPanel appShortcuts={appShortcuts} onModelChange={onModelChange} selectedModels={{}} view={view} />
    </I18nProvider>,
  );
}

function createMarketItem(id: string, installed: boolean) {
  const displayName = id === "opencode" ? "OpenCode" : id === "gemini" ? "Gemini CLI" : "Codex CLI";
  return {
    id,
    catalogVersion: "fixture-catalog",
    displayName,
    description: "Fixture Agent",
    protocol: "acp",
    version: "1.0.0",
    coreCompatible: true,
    capabilities: {
      purposes: ["text"],
      textPrompt: true,
      modelDiscovery: true,
    },
    verification: {
      status: "tested",
      testedAt: "2026-08-17T00:00:00Z",
      evidenceId: "fixture-evidence",
    },
    distributions: [{
      distributionId: "fixture-binary",
      distributionType: "binary",
      selectable: true,
      recommended: true,
      ownership: "managed",
      reasonCode: null,
      requiredRuntime: null,
      resolvedVersion: "1.0.0",
      downloadSize: null,
      targetPath: null,
    }],
    recommendedDistributionId: "fixture-binary",
    installed: installed ? {
      agentId: id,
      displayName,
      version: "1.0.0",
      protocol: "acp",
      distributionId: "fixture-binary",
      distributionType: "binary",
      ownership: "managed",
      displayInstallPath: `/tmp/${id}`,
      enabled: true,
      installed: true,
      installationStatus: "installed",
      runtimeStatus: "ready",
      protocolStatus: "ready",
      connected: false,
      executionReady: true,
      healthStale: false,
      selectedModelId: null,
      modelStatus: null,
      updateAvailable: false,
      operation: null,
      lastCheckedAt: null,
      error: null,
      warnings: [],
    } : null,
    updateAvailable: false,
  };
}
