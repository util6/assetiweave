/* @vitest-environment jsdom */

import { fireEvent, render, screen } from "@testing-library/react";
import { Bot } from "lucide-react";
import { describe, expect, it, vi } from "vitest";
import { I18nProvider } from "../../i18n/I18nProvider";
import type { AgentUninstallPreview } from "../../services/agentRuntime";
import { AgentUninstallPreviewDialog } from "./AgentUninstallPreviewDialog";

const preview: AgentUninstallPreview = {
  agentId: "fixture-agent",
  currentInstallation: {
    agentId: "fixture-agent",
    displayName: "Fixture Agent",
    version: "1.2.3",
    protocol: "acp",
    distributionId: "fixture-system",
    distributionType: "system",
    ownership: "system",
    capabilities: {
      purposes: ["text"],
      textPrompt: true,
      modelDiscovery: false,
      resume: true,
      historyReplay: true,
      liveEvents: true,
      richHistoryReplay: false,
      teamTools: true,
      resumeArgs: null,
    },
    displayInstallPath: "/usr/local/bin/fixture-agent",
    enabled: true,
    installed: true,
    installationStatus: "installed",
    runtimeStatus: "ready",
    protocolStatus: "unknown",
    connected: false,
    executionReady: false,
    healthStale: true,
    selectedModelId: null,
    modelStatus: null,
    updateAvailable: false,
    operation: null,
    lastCheckedAt: null,
    error: null,
    warnings: [],
  },
  ownership: "system",
  targetPath: "/usr/local/bin/fixture-agent",
  capabilityAssignments: ["translation", "memory"],
  conflicts: ["assignment:translation", "assignment:memory"],
  warnings: [],
  confirmationRequired: true,
  previewToken: "fixture-token",
};

describe("AgentUninstallPreviewDialog", () => {
  it("requires all capability assignments to be selected before confirming", () => {
    const onConfirm = vi.fn();
    render(
      <I18nProvider>
        <AgentUninstallPreviewDialog
          agent={{
            id: "fixture-agent",
            name: "Fixture Agent",
            command: "/usr/local/bin/fixture-agent",
            protocol: "ACP",
            description: "Fixture",
            icon: Bot,
            connectionMode: "registry",
            installed: preview.currentInstallation,
          }}
          onClose={vi.fn()}
          onConfirm={onConfirm}
          preview={preview}
        />
      </I18nProvider>,
    );

    const confirm = screen.getByRole("button", { name: "Confirm uninstall" });
    expect((confirm as HTMLButtonElement).disabled).toBe(true);
    fireEvent.click(screen.getByLabelText("translation"));
    expect((confirm as HTMLButtonElement).disabled).toBe(true);
    fireEvent.click(screen.getByLabelText("memory"));
    expect((confirm as HTMLButtonElement).disabled).toBe(false);
    fireEvent.click(confirm);
    expect(onConfirm).toHaveBeenCalledWith(["translation", "memory"]);
  });
});
