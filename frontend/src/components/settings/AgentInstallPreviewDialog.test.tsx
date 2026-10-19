/* @vitest-environment jsdom */

import { cleanup, render, screen } from "@testing-library/react";
import { Bot } from "lucide-react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { I18nProvider } from "../../i18n/I18nProvider";
import type { AgentInstallPreview } from "../../services/agentRuntime";
import { AgentInstallPreviewDialog } from "./AgentInstallPreviewDialog";
import type { AgentCatalogItem } from "./agentCatalog";

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

const mockAgent: AgentCatalogItem = {
  id: "antigravity",
  name: "Google Antigravity",
  command: "antigravity",
  protocol: "acp",
  description: "Antigravity official ACP server",
  icon: Bot,
  connectionMode: "registry",
  updateAvailable: true,
  hasSelectableDistribution: true,
};

function createPreview(action: "install" | "update" | "reinstall" = "update"): AgentInstallPreview {
  return {
    agentId: "antigravity",
    catalogVersion: "2026-09-07",
    action,
    selectedDistribution: {
      distributionId: "binary-darwin-aarch64",
      distributionType: "binary",
      selectable: true,
      recommended: true,
      ownership: "managed",
      reasonCode: null,
      requiredRuntime: null,
      resolvedVersion: "1.4.0",
      downloadSize: 316047360,
      targetPath: null,
    },
    alternatives: [
      {
        distributionId: "system-antigravity",
        distributionType: "system",
        selectable: false,
        recommended: false,
        ownership: "system",
        reasonCode: "runtime_missing",
        requiredRuntime: "antigravity",
        resolvedVersion: null,
        downloadSize: null,
        targetPath: null,
      },
    ],
    currentInstallation:
      action !== "install"
        ? {
            agentId: "antigravity",
            displayName: "Google Antigravity",
            version: "1.1.1",
            protocol: "acp",
            distributionId: "binary-darwin-aarch64",
            distributionType: "binary",
            ownership: "managed",
            capabilities: {
              purposes: ["text"],
              textPrompt: true,
              modelDiscovery: false,
              resume: false,
              historyReplay: false,
              liveEvents: false,
              richHistoryReplay: false,
              resumeArgs: null,
            },
            displayInstallPath: "/tmp/antigravity",
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
            updateAvailable: true,
            operation: null,
            lastCheckedAt: null,
            error: null,
            warnings: [],
          }
        : null,
    targetVersion: "1.4.0",
    ownership: "managed",
    targetPath: "/tmp/antigravity",
    downloadSize: 316047360,
    runtimeRequirements: [],
    conflicts: [],
    warnings: [],
    confirmationRequired: true,
    previewToken: "token-123",
  };
}

describe("AgentInstallPreviewDialog", () => {
  it("renders clear version evolution flow from current version to upgrade target", () => {
    const preview = createPreview("update");
    render(
      <I18nProvider>
        <AgentInstallPreviewDialog
          agent={mockAgent}
          onClose={vi.fn()}
          onConfirm={vi.fn()}
          onSelectDistribution={vi.fn()}
          preview={preview}
        />
      </I18nProvider>,
    );

    expect(screen.getByText(/当前版本 v1.1.1/)).toBeTruthy();
    expect(screen.getByText(/升级至 v1.4.0/)).toBeTruthy();
    expect(screen.getByText("➔")).toBeTruthy();
    expect(screen.getByText(/版本 v1.4.0/)).toBeTruthy();
    expect(screen.getByText("已选择")).toBeTruthy();
  });

  it("renders install version badge when installing fresh", () => {
    const preview = createPreview("install");
    render(
      <I18nProvider>
        <AgentInstallPreviewDialog
          agent={mockAgent}
          onClose={vi.fn()}
          onConfirm={vi.fn()}
          onSelectDistribution={vi.fn()}
          preview={preview}
        />
      </I18nProvider>,
    );

    expect(screen.queryByText(/当前版本/)).toBeNull();
    expect(screen.getByText(/安装版本 v1.4.0/)).toBeTruthy();
    expect(screen.getByText("已选择")).toBeTruthy();
  });

  it("renders reinstall version badges when reinstalling", () => {
    const preview = createPreview("reinstall");
    render(
      <I18nProvider>
        <AgentInstallPreviewDialog
          agent={mockAgent}
          onClose={vi.fn()}
          onConfirm={vi.fn()}
          onSelectDistribution={vi.fn()}
          preview={preview}
        />
      </I18nProvider>,
    );

    expect(screen.getByText(/当前版本 v1.1.1/)).toBeTruthy();
    expect(screen.getByText(/重装版本 v1.4.0/)).toBeTruthy();
  });
});
