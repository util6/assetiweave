/* @vitest-environment jsdom */

import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { I18nProvider } from "../../i18n/I18nProvider";
import { AgentSettingsPanel } from "./AgentSettingsPanel";
import type { AppShortcut } from "../../types";

const agentRuntime = vi.hoisted(() => ({
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
    const geminiRow = screen.getByRole("heading", { name: "Gemini CLI" }).closest("article");
    expect(openCodeRow?.querySelector('path[d="M16 6H8v12h8V6zm4 16H4V2h16v20z"]')).toBeTruthy();
    expect(geminiRow?.querySelector('path[d^="M20.616 10.835"]')).toBeTruthy();
    expect(await screen.findByText("可用")).toBeTruthy();
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
    renderPanel();

    const geminiRow = screen.getByRole("heading", { name: "Gemini CLI" }).closest("article");
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
    renderPanel();

    const codexRow = screen.getByRole("heading", { name: "Codex CLI" }).closest("article");
    fireEvent.click(within(codexRow as HTMLElement).getByRole("button", { name: "测试连接" }));

    await waitFor(() => expect(agentRuntime.checkAgentConnection).toHaveBeenCalledWith("codex", "connection"));
    await waitFor(() => expect(within(codexRow as HTMLElement).getByTitle("codex-acp 1.1.2")).toBeTruthy());
  });

  it("loads ACP models in a dialog and persists the selected model callback", async () => {
    const onModelChange = vi.fn();
    renderPanel({ onModelChange });

    const codexRow = screen.getByRole("heading", { name: "Codex CLI" }).closest("article");
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

  it("opens the custom Agent definition template from the add menu", () => {
    renderPanel();

    fireEvent.click(screen.getByRole("button", { name: /添加自定义 Agent/ }));
    fireEvent.click(screen.getByRole("menuitem", { name: /查看定义模板/ }));

    expect(screen.getByRole("heading", { name: "自定义 Agent 定义模板" })).toBeTruthy();
    expect(screen.getByText(/暂不把自定义定义写入运行时 Registry/)).toBeTruthy();
  });
});

function renderPanel({
  appShortcuts = [],
  onModelChange = vi.fn(),
}: {
  appShortcuts?: AppShortcut[];
  onModelChange?: (agentId: string, modelId: string) => void;
} = {}) {
  return render(
    <I18nProvider>
      <AgentSettingsPanel appShortcuts={appShortcuts} onModelChange={onModelChange} selectedModels={{}} />
    </I18nProvider>,
  );
}
