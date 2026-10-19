/* @vitest-environment jsdom */

import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { I18nProvider, useI18n } from "../../i18n/I18nProvider";
import { AgentConnectionRow } from "./AgentConnectionRow";
import type { AgentCatalogItem } from "./agentCatalog";
import type { AgentLifecycleTaskSnapshot } from "../../services/agentRuntime";
import type { ComponentProps } from "react";

beforeEach(() => {
  vi.stubGlobal("localStorage", {
    getItem: () => "zh",
    setItem: vi.fn(),
  });
  vi.stubGlobal("navigator", { language: "zh-CN" });
});

afterEach(() => {
  cleanup();
});

import { Bot } from "lucide-react";

function createAgent(id = "opencode"): AgentCatalogItem {
  return {
    id,
    name: "OpenCode",
    description: "OpenCode description",
    command: id,
    protocol: "acp",
    icon: Bot,
    connectionMode: "registry",
    installed: {
      agentId: id,
      displayName: "OpenCode",
      version: "1.0.0",
      protocol: "acp",
      distributionId: "fixture-binary",
      distributionType: "binary",
      ownership: "managed",
      displayInstallPath: `/tmp/${id}`,
      capabilities: {
        purposes: ["text"],
        textPrompt: true,
        modelDiscovery: true,
        resume: true,
        historyReplay: true,
        liveEvents: true,
        richHistoryReplay: false,
        resumeArgs: null,
      },
      enabled: true,
      installed: true,
      installationStatus: "installed",
      runtimeStatus: "ready",
      protocolStatus: "ready",
      connected: true,
      executionReady: true,
      healthStale: false,
      selectedModelId: null,
      modelStatus: null,
      updateAvailable: false,
      operation: null,
      lastCheckedAt: null,
      error: null,
      warnings: [],
    },
    updateAvailable: false,
    hasSelectableDistribution: true,
  };
}

function createLifecycleTask(
  phase: AgentLifecycleTaskSnapshot["phase"],
  downloadedBytes: number | null = null,
  totalBytes: number | null = null,
  state: AgentLifecycleTaskSnapshot["state"] = "running",
  error: AgentLifecycleTaskSnapshot["error"] = null,
): AgentLifecycleTaskSnapshot {
  return {
    id: "task-1",
    agentId: "opencode",
    action: "install",
    state,
    phase,
    catalogVersion: "1.0.0",
    agentVersion: "1.0.0",
    distributionId: "binary-1",
    distributionType: "binary",
    ownership: "managed",
    progress: {
      completedUnits: 1,
      totalUnits: 3,
      downloadedBytes,
      totalBytes,
    },
    cancellable: true,
    createdAt: new Date().toISOString(),
    updatedAt: new Date().toISOString(),
    finishedAt: null,
    result: null,
    error,
    warnings: [],
  };
}

function TestRow(props: Partial<ComponentProps<typeof AgentConnectionRow>>) {
  const { t } = useI18n();
  const agent = props.agent ?? createAgent();
  return (
    <AgentConnectionRow
      agent={agent}
      connectionState="available"
      isTesting={false}
      onInstall={vi.fn()}
      t={t}
      view="market"
      {...props}
    />
  );
}

describe("AgentConnectionRow lifecycle progress & error classification", () => {
  it("renders downloading progress percentage and size tooltip when in download phase", () => {
    const task = createLifecycleTask("downloading", 4500000, 10000000);
    render(
      <I18nProvider>
        <TestRow lifecycleTask={task} />
      </I18nProvider>,
    );

    const button = screen.getByRole("button", { name: /下载中 45%/ });
    expect(button).toBeTruthy();
    expect((button as HTMLButtonElement).disabled).toBe(true);
    expect(button.getAttribute("title")).toContain("4.3 MiB / 9.5 MiB");
  });

  it("renders installing text when in installing phase", () => {
    const task = createLifecycleTask("installing");
    render(
      <I18nProvider>
        <TestRow lifecycleTask={task} />
      </I18nProvider>,
    );

    const button = screen.getByRole("button", { name: "正在安装..." });
    expect(button).toBeTruthy();
    expect((button as HTMLButtonElement).disabled).toBe(true);
  });

  it("renders ACP verification text when in probing_protocol phase", () => {
    const task = createLifecycleTask("probing_protocol");
    render(
      <I18nProvider>
        <TestRow lifecycleTask={task} />
      </I18nProvider>,
    );

    const button = screen.getByRole("button", { name: "验证 ACP..." });
    expect(button).toBeTruthy();
    expect((button as HTMLButtonElement).disabled).toBe(true);
  });

  it("renders classified badge for upstream version not found (404)", () => {
    const task = createLifecycleTask(
      "failed",
      null,
      null,
      "failed",
      {
        code: "upstream_version_not_found",
        message: "HTTP 404 Not Found",
        agentId: "opencode",
        phase: "download",
        retryable: false,
        action: null,
      },
    );
    render(
      <I18nProvider>
        <TestRow connectionState="failed" lifecycleTask={task} />
      </I18nProvider>,
    );

    expect(screen.getByText("源头无此版本")).toBeTruthy();
    expect(screen.getByText(/HTTP 404 Not Found/)).toBeTruthy();
  });

  it("renders classified badge for network error", () => {
    const task = createLifecycleTask(
      "failed",
      null,
      null,
      "failed",
      {
        code: "network_error",
        message: "connection timed out",
        agentId: "opencode",
        phase: "download",
        retryable: true,
        action: null,
      },
    );
    render(
      <I18nProvider>
        <TestRow connectionState="failed" lifecycleTask={task} />
      </I18nProvider>,
    );

    expect(screen.getByText("网络连接异常")).toBeTruthy();
    expect(screen.getByText(/connection timed out/)).toBeTruthy();
  });

  it("renders classified badge for install failed", () => {
    const task = createLifecycleTask(
      "failed",
      null,
      null,
      "failed",
      {
        code: "install_failed",
        message: "npm install failed with exit code 1",
        agentId: "opencode",
        phase: "install",
        retryable: false,
        action: null,
      },
    );
    render(
      <I18nProvider>
        <TestRow connectionState="failed" lifecycleTask={task} />
      </I18nProvider>,
    );

    expect(screen.getByText("安装失败")).toBeTruthy();
    expect(screen.getByText(/npm install failed with exit code 1/)).toBeTruthy();
  });

  it("renders classified badge for protocol handshake failure", () => {
    const task = createLifecycleTask(
      "failed",
      null,
      null,
      "failed",
      {
        code: "protocol_handshake_failed",
        message: "ACP initialize response timed out",
        agentId: "opencode",
        phase: "verify",
        retryable: false,
        action: null,
      },
    );
    render(
      <I18nProvider>
        <TestRow connectionState="failed" lifecycleTask={task} />
      </I18nProvider>,
    );

    expect(screen.getByText("ACP 握手失败")).toBeTruthy();
    expect(screen.getByText(/ACP initialize response timed out/)).toBeTruthy();
  });
});
