/* @vitest-environment jsdom */

import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { I18nProvider } from "../../i18n/I18nProvider";
import type { ConversationAdapterPackageCatalogEntry } from "../../services/conversations";
import { ConversationScriptResourcePanel } from "./ConversationScriptResourcePanel";

const serviceMocks = vi.hoisted(() => ({
  getTask: vi.fn(),
  inspect: vi.fn(),
  install: vi.fn(),
  list: vi.fn(),
  listReleases: vi.fn(),
  prepare: vi.fn(),
  unregister: vi.fn(),
  uninstall: vi.fn(),
  update: vi.fn(),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: vi.fn().mockResolvedValue(() => {}),
}));

vi.mock("../../services/conversations", async () => {
  const actual = await vi.importActual<typeof import("../../services/conversations")>(
    "../../services/conversations",
  );
  return {
    ...actual,
    getConversationAdapterPackageTask: serviceMocks.getTask,
    inspectConversationAdapterPackage: serviceMocks.inspect,
    installConversationAdapterPackage: serviceMocks.install,
    listConversationAdapterPackageReleases: serviceMocks.listReleases,
    listConversationAdapterPackages: serviceMocks.list,
    prepareConversationAdapterPackageChange: serviceMocks.prepare,
    unregisterConversationAdapter: serviceMocks.unregister,
    uninstallConversationAdapterPackage: serviceMocks.uninstall,
    updateConversationAdapterPackage: serviceMocks.update,
  };
});

describe("ConversationScriptResourcePanel", () => {
  beforeEach(() => {
    serviceMocks.getTask.mockReset().mockResolvedValue(null);
    serviceMocks.list.mockReset().mockResolvedValue(entries);
    serviceMocks.inspect.mockReset().mockResolvedValue({
      origin: "managed_release",
      package: entries[1].installed_package,
      adapter: entries[1].installed_adapter,
      affected_sources: [{
        id: "codex-live",
        adapter_id: "codex",
        name: "Codex Live",
        kind: "directory",
        location: "/tmp/codex",
        enabled: true,
        created_at: "2026-07-15T00:00:00Z",
        updated_at: "2026-07-15T00:00:00Z",
      }],
    });
    serviceMocks.listReleases.mockReset().mockResolvedValue([release]);
    serviceMocks.prepare.mockReset().mockResolvedValue({
      action: "update",
      origin: "managed_release",
      package_id: "io.github.util6.codex-session",
      adapter_id: "codex",
      managed_paths: ["/tmp/packages/io.github.util6.codex-session"],
      affected_sources: [],
      task_conflicts: [],
      preserves_conversation_records: true,
      risk: "high_risk_write",
      confirmation_required: true,
    });
    serviceMocks.update.mockReset().mockResolvedValue({
      id: "update-1",
      status: "running",
      action: "update",
      item_id: "io.github.util6.codex-session",
      package_id: "io.github.util6.codex-session",
      version: "1.0.1",
      catalog_url: null,
      dry_run: false,
      phase: "updating",
      started_at: "2026-07-15T00:00:00Z",
      finished_at: null,
      result: null,
      error: null,
    });
  });

  afterEach(() => {
    cleanup();
    vi.clearAllMocks();
  });

  it("separates connected, update, and discover views", async () => {
    renderPanel();

    expect(await screen.findByText("Built-in Codex")).toBeTruthy();
    expect(screen.getByText("Codex Session Parser")).toBeTruthy();
    expect(screen.queryByText("Qwen Web Harvester")).toBeNull();

    fireEvent.click(screen.getByRole("tab", { name: /Updates \(1\)/ }));
    expect(screen.getByText("Codex Session Parser")).toBeTruthy();
    expect(screen.queryByText("Built-in Codex")).toBeNull();

    fireEvent.click(screen.getByRole("tab", { name: /Discover \(1\)/ }));
    expect(screen.getByText("Qwen Web Harvester")).toBeTruthy();
  });

  it("shows an accessible loading indicator while the catalog is validated", async () => {
    let resolveCatalog: (value: ConversationAdapterPackageCatalogEntry[]) => void = () => undefined;
    serviceMocks.list.mockReturnValueOnce(new Promise((resolve) => {
      resolveCatalog = resolve;
    }));

    renderPanel();

    expect(screen.getByRole("status").textContent).toContain("Validating script directories");
    resolveCatalog(entries);
    expect(await screen.findByText("Built-in Codex")).toBeTruthy();
  });

  it("shows runtime details, version history, and changelog", async () => {
    renderPanel();
    const details = await screen.findAllByRole("button", { name: "Details" });
    fireEvent.click(details[1]);

    expect(await screen.findByText("io.github.util6.codex-session")).toBeTruthy();
    expect(screen.getByText("Codex Live · codex-live")).toBeTruthy();
    expect(screen.getByText(/Improve Codex session parsing compatibility/)).toBeTruthy();
    expect((screen.getByRole("combobox", { name: "Select install version" }) as HTMLSelectElement).value)
      .toBe("1.0.1");
  });

  it("does not execute an update until preflight is explicitly confirmed", async () => {
    renderPanel();
    fireEvent.click(await screen.findByRole("tab", { name: /Updates \(1\)/ }));
    fireEvent.click(screen.getByRole("button", { name: "Update" }));

    await waitFor(() => expect(serviceMocks.prepare).toHaveBeenCalledTimes(1));
    expect(serviceMocks.update).not.toHaveBeenCalled();
    const updateButtons = await screen.findAllByRole("button", { name: "Update" });
    fireEvent.click(updateButtons[updateButtons.length - 1]);

    await waitFor(() => {
      expect(serviceMocks.update).toHaveBeenCalledWith({
        packageId: "io.github.util6.codex-session",
        version: undefined,
        confirmed: true,
      });
    });
  });
});

function renderPanel() {
  return render(
    <I18nProvider>
      <ConversationScriptResourcePanel
        onInstalled={vi.fn()}
        onManifestSelect={vi.fn()}
        onNotify={vi.fn()}
        onNotifyError={vi.fn()}
        recordKind="session"
      />
    </I18nProvider>,
  );
}

const entries: ConversationAdapterPackageCatalogEntry[] = [
  {
    item: {
      id: "builtin-codex",
      name: "Built-in Codex",
      version: "0.5.1",
      record_kind: "session",
      provider: "built_in",
      adapter_id: "builtin-codex",
      tags: [],
      source: { type: "local_directory", url: "" },
    },
    installed: true,
    update_available: false,
    runtime_ready: true,
    status: "built_in",
    installed_adapter: {
      id: "builtin-codex",
      name: "Built-in Codex",
      kind: "external",
      version: "0.5.1",
      enabled: true,
      trust_state: "built_in",
      capabilities: ["read_session"],
      input_kinds: ["directory"],
      created_at: "2026-07-15T00:00:00Z",
      updated_at: "2026-07-15T00:00:00Z",
    },
  },
  {
    item: {
      id: "io.github.util6.codex-session",
      name: "Codex Session Parser",
      version: "1.0.1",
      record_kind: "session",
      provider: "util6",
      adapter_id: "codex",
      tags: [],
      source: { type: "artifact_zip", url: "https://example.test/codex.zip" },
    },
    installed: true,
    update_available: true,
    runtime_ready: true,
    status: "update_available",
    installed_package: {
      package_id: "io.github.util6.codex-session",
      adapter_id: "codex",
      name: "Codex Session Parser",
      version: "1.0.0",
      record_kind: "session",
      install_dir: "/tmp/packages/io.github.util6.codex-session/versions/1.0.0",
      manifest_path: "/tmp/package/conversation-adapter-package.json",
      adapter_manifest_path: "/tmp/package/conversation-adapter.json",
      runtime_protocol: "stdio-ndjson-v1",
      runtime_ready: true,
      origin: "managed_release",
      update_policy: "manual",
      latest_version: "1.0.1",
      runtime_gate_status: "ready",
      installed_content_hash: "content-hash",
      trusted_package_hash: "trusted-hash",
      created_at: "2026-07-15T00:00:00Z",
      updated_at: "2026-07-15T00:00:00Z",
    },
    installed_adapter: {
      id: "codex",
      name: "Codex",
      kind: "external",
      version: "1.0.0",
      enabled: true,
      manifest_path: "/tmp/package/conversation-adapter.json",
      trust_state: "trusted",
      capabilities: ["read_session"],
      input_kinds: ["directory"],
      created_at: "2026-07-15T00:00:00Z",
      updated_at: "2026-07-15T00:00:00Z",
    },
  },
  {
    item: {
      id: "io.github.util6.qwen-session",
      name: "Qwen Web Harvester",
      version: "0.1.0",
      record_kind: "session",
      provider: "util6",
      adapter_id: "qwen",
      tags: [],
      source: { type: "artifact_zip", url: "https://example.test/qwen.zip" },
    },
    installed: false,
    update_available: false,
    runtime_ready: false,
    status: "not_installed",
  },
];

const release = {
  catalog_url: "https://example.test/index.json",
  package_id: "io.github.util6.codex-session",
  adapter_id: "codex",
  name: "Codex Session Parser",
  publisher: "util6",
  version: "1.0.1",
  channel: "stable" as const,
  released_at: "2026-07-15T00:00:00Z",
  core_compatibility: ">=0.5.0, <1.0.0",
  artifact_url: "https://example.test/codex.zip",
  artifact_size: 100,
  artifact_sha256: "a".repeat(64),
  changelog_markdown: "## 1.0.1\n\n- Improve Codex session parsing compatibility.",
  breaking_change: false,
  runtime_protocol: "stdio-ndjson-v1",
  record_kind: "session" as const,
  package_manifest_file: "conversation-adapter-package.json",
  adapter_manifest_file: "conversation-adapter.json",
  fetched_at: "2026-07-15T00:00:00Z",
};
