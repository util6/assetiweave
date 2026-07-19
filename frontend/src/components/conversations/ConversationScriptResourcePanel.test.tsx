/* @vitest-environment jsdom */

import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { I18nProvider } from "../../i18n/I18nProvider";
import type { ConversationAdapterPackageCatalogEntry } from "../../services/conversations";
import { ConversationScriptResourcePanel } from "./ConversationScriptResourcePanel";

const serviceMocks = vi.hoisted(() => ({
  checkUpdates: vi.fn(),
  deleteVersion: vi.fn(),
  getTask: vi.fn(),
  inspect: vi.fn(),
  install: vi.fn(),
  list: vi.fn(),
  listReleases: vi.fn(),
  listVersions: vi.fn(),
  prepare: vi.fn(),
  register: vi.fn(),
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
    checkConversationAdapterPackageUpdates: serviceMocks.checkUpdates,
    deleteConversationAdapterPackageVersion: serviceMocks.deleteVersion,
    getConversationAdapterPackageTask: serviceMocks.getTask,
    inspectConversationAdapterPackage: serviceMocks.inspect,
    installConversationAdapterPackage: serviceMocks.install,
    listConversationAdapterPackageReleases: serviceMocks.listReleases,
    listConversationAdapterPackages: serviceMocks.list,
    listInstalledConversationAdapterPackageVersions: serviceMocks.listVersions,
    prepareConversationAdapterPackageChange: serviceMocks.prepare,
    registerConversationAdapter: serviceMocks.register,
    unregisterConversationAdapter: serviceMocks.unregister,
    uninstallConversationAdapterPackage: serviceMocks.uninstall,
    updateConversationAdapterPackage: serviceMocks.update,
  };
});

describe("ConversationScriptResourcePanel", () => {
  beforeEach(() => {
    serviceMocks.checkUpdates.mockReset().mockResolvedValue([]);
    serviceMocks.deleteVersion.mockReset().mockResolvedValue({ deleted: true });
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
    serviceMocks.listVersions.mockReset().mockResolvedValue([]);
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
    serviceMocks.register.mockReset().mockResolvedValue({});
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
    serviceMocks.install.mockReset().mockResolvedValue({
      id: "install-1",
      status: "running",
      action: "install",
      item_id: "io.github.util6.qwen-session",
      package_id: "io.github.util6.qwen-session",
      version: "0.1.0",
      catalog_url: null,
      dry_run: false,
      phase: "installing",
      started_at: "2026-07-15T00:00:00Z",
      finished_at: null,
      result: null,
      error: null,
    });
    serviceMocks.uninstall.mockReset().mockResolvedValue({
      id: "uninstall-1",
      status: "running",
      action: "uninstall",
      item_id: "io.github.util6.codex-session",
      package_id: "io.github.util6.codex-session",
      version: "1.0.0",
      catalog_url: null,
      dry_run: false,
      phase: "uninstalling",
      started_at: "2026-07-15T00:00:00Z",
      finished_at: null,
      result: null,
      error: null,
    });
    serviceMocks.unregister.mockReset().mockResolvedValue({ unregistered: true });
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

  it("renders backend-provided portable manifest paths", async () => {
    renderPanel();

    expect(
      await screen.findByText("~/conversation-adapters/codex/conversation-adapter.json"),
    ).toBeTruthy();
    expect(screen.queryByText("/tmp/package/conversation-adapter.json")).toBeNull();
  });

  it("shows installed adapters that are ahead of the catalog without offering an update", async () => {
    const aheadEntry: ConversationAdapterPackageCatalogEntry = {
      ...entries[1],
      update_available: false,
      ahead_of_release: true,
      status: "ahead_of_release",
      installed_package: {
        ...entries[1].installed_package!,
        version: "1.1.0",
      },
      installed_adapter: {
        ...entries[1].installed_adapter!,
        version: "1.1.0",
      },
    };
    serviceMocks.list.mockResolvedValueOnce([aheadEntry]);

    renderPanel();

    expect(await screen.findByText("Ahead")).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Update" })).toBeNull();
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

  it("checks for updates explicitly and opens the updates view", async () => {
    serviceMocks.checkUpdates.mockResolvedValueOnce([{
      package_id: "io.github.util6.codex-session",
      current_version: "1.0.0",
      latest_compatible_release: release,
      update_available: true,
    }]);
    renderPanel();

    fireEvent.click(await screen.findByRole("button", { name: "Check for updates" }));

    await waitFor(() => expect(serviceMocks.checkUpdates).toHaveBeenCalledWith({ force: true }));
    expect(serviceMocks.list).toHaveBeenCalledTimes(2);
    expect(screen.getByRole("tab", { name: /Updates \(1\)/ }).getAttribute("aria-selected")).toBe("true");
  });

  it("registers a discovered package only after explicit confirmation", async () => {
    renderPanel();
    fireEvent.click(await screen.findByRole("tab", { name: /Discover \(1\)/ }));
    fireEvent.click(screen.getByRole("button", { name: "Register" }));

    await waitFor(() => expect(serviceMocks.prepare).toHaveBeenCalledWith({
      action: "install",
      packageId: "io.github.util6.qwen-session",
      adapterId: "qwen",
    }));
    expect(serviceMocks.install).not.toHaveBeenCalled();
    const registerButtons = screen.getAllByRole("button", { name: "Register" });
    fireEvent.click(registerButtons[registerButtons.length - 1]);

    await waitFor(() => expect(serviceMocks.install).toHaveBeenCalledWith({
      packageId: "io.github.util6.qwen-session",
      version: undefined,
      confirmed: true,
    }));
  });

  it("registers a valid package discovered in the local adapter directory", async () => {
    const localEntry: ConversationAdapterPackageCatalogEntry = {
      item: {
        id: "local.session-parser",
        name: "Local Session Parser",
        version: "1.0.0",
        record_kind: "session",
        provider: "local_directory",
        adapter_id: "local-session",
        tags: [],
        manifest_file: "conversation-adapter.json",
        source: {
          type: "local_directory",
          url: "/tmp/conversation-adapters/local-session",
        },
      },
      installed: false,
      update_available: false,
      ahead_of_release: false,
      runtime_ready: false,
      status: "not_installed",
      install_path: "/tmp/conversation-adapters/local-session",
      display_install_path: "~/.assetiweave/conversation-adapters/local-session",
      display_manifest_path: "~/.assetiweave/conversation-adapters/local-session/conversation-adapter.json",
    };
    serviceMocks.list.mockResolvedValueOnce([localEntry]);

    renderPanel();
    fireEvent.click(await screen.findByRole("tab", { name: /Discover \(1\)/ }));
    fireEvent.click(screen.getByRole("button", { name: "Register" }));

    await waitFor(() => expect(serviceMocks.register).toHaveBeenCalledWith(
      "/tmp/conversation-adapters/local-session/conversation-adapter.json",
      false,
      true,
    ));
    expect(serviceMocks.prepare).not.toHaveBeenCalled();
    expect(serviceMocks.install).not.toHaveBeenCalled();
  });

  it("uninstalls a managed runtime without using the delete action", async () => {
    serviceMocks.list.mockResolvedValueOnce([entries[1]]);
    renderPanel();
    expect(await screen.findByRole("button", { name: "Manage / delete" })).toBeTruthy();
    fireEvent.click(await screen.findByRole("button", { name: "Uninstall" }));
    const confirmDialog = await screen.findByRole("dialog", { name: "Confirm plugin change" });
    fireEvent.click(within(confirmDialog).getByRole("button", { name: "Uninstall" }));

    await waitFor(() => expect(serviceMocks.uninstall).toHaveBeenCalledWith({
      packageId: "io.github.util6.codex-session",
      confirmed: true,
    }));
    expect(serviceMocks.deleteVersion).not.toHaveBeenCalled();
  });

  it("opens managed version deletion from the market row", async () => {
    renderPanel();

    fireEvent.click(await screen.findByRole("button", { name: "Manage / delete" }));

    expect(await screen.findByRole("heading", { name: "Installed offline versions" })).toBeTruthy();
    expect(screen.getByText("Uninstall the running version before deleting it.")).toBeTruthy();
  });

  it("presents external runtime unregistering as uninstall while retaining its files", async () => {
    const localEntry: ConversationAdapterPackageCatalogEntry = {
      ...entries[1],
      update_available: false,
      status: "local_registered",
      installed_package: {
        ...entries[1].installed_package!,
        package_id: "com.util6.zcode-local",
        adapter_id: "zcode",
        origin: "local_directory",
        install_dir: "/Users/example/zcode-adapter",
      },
      installed_adapter: {
        ...entries[1].installed_adapter!,
        id: "zcode",
      },
    };
    serviceMocks.list.mockResolvedValueOnce([localEntry]);
    renderPanel();

    expect(screen.queryByRole("button", { name: "Manage / delete" })).toBeNull();
    fireEvent.click(await screen.findByRole("button", { name: "Uninstall" }));
    const confirmDialog = await screen.findByRole("dialog", { name: "Confirm plugin change" });
    fireEvent.click(within(confirmDialog).getByRole("button", { name: "Uninstall" }));

    await waitFor(() => expect(serviceMocks.unregister).toHaveBeenCalledWith({
      adapterId: "zcode",
      confirmed: true,
    }));
    expect(serviceMocks.uninstall).not.toHaveBeenCalled();
    expect(serviceMocks.deleteVersion).not.toHaveBeenCalled();
  });

  it("offers uninstall for a built-in runtime and disables it through unregister", async () => {
    serviceMocks.list.mockResolvedValueOnce([entries[0]]);
    renderPanel();

    expect(screen.queryByRole("button", { name: "Manage / delete" })).toBeNull();
    fireEvent.click(await screen.findByRole("button", { name: "Uninstall" }));
    const confirmDialog = await screen.findByRole("dialog", { name: "Confirm plugin change" });
    fireEvent.click(within(confirmDialog).getByRole("button", { name: "Uninstall" }));

    await waitFor(() => expect(serviceMocks.unregister).toHaveBeenCalledWith({
      adapterId: "builtin-codex",
      confirmed: true,
    }));
  });

  it("shows a disabled built-in runtime as uninstalled without delete actions", async () => {
    serviceMocks.list.mockResolvedValueOnce([{
      ...entries[0],
      runtime_ready: false,
      status: "uninstalled",
      installed_adapter: {
        ...entries[0].installed_adapter!,
        enabled: false,
        manifest_path: "/tmp/builtin/conversation-adapter.json",
      },
    }]);
    renderPanel();

    expect(await screen.findByText("Uninstalled (files and records retained)")).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Uninstall" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Manage / delete" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Delete version" })).toBeNull();
  });

  it("keeps a register entry for an uninstalled built-in runtime", async () => {
    serviceMocks.list.mockResolvedValueOnce([{
      ...entries[0],
      runtime_ready: false,
      status: "uninstalled",
      installed_adapter: {
        ...entries[0].installed_adapter!,
        enabled: false,
        manifest_path: "/tmp/builtin/conversation-adapter.json",
      },
    }]);
    renderPanel();

    fireEvent.click(await screen.findByRole("button", { name: "Register" }));

    await waitFor(() => expect(serviceMocks.register).toHaveBeenCalledWith(
      "/tmp/builtin/conversation-adapter.json",
      false,
      true,
    ));
  });

  it("allows deleting the last installed version after its runtime is uninstalled", async () => {
    const uninstalledEntry: ConversationAdapterPackageCatalogEntry = {
      ...entries[1],
      runtime_ready: false,
      status: "uninstalled",
      installed_adapter: null,
      installed_package: {
        ...entries[1].installed_package!,
        runtime_ready: false,
        runtime_gate_status: "runtime_missing",
      },
    };
    serviceMocks.list.mockResolvedValueOnce([uninstalledEntry]);
    serviceMocks.inspect.mockResolvedValueOnce({
      origin: "managed_release",
      package: uninstalledEntry.installed_package,
      adapter: null,
      affected_sources: [],
    });
    serviceMocks.listVersions.mockResolvedValueOnce([{
      package_id: "io.github.util6.codex-session",
      version: "1.0.0",
      install_dir: "/tmp/packages/io.github.util6.codex-session/versions/1.0.0",
      artifact_hash: "artifact-hash",
      content_hash: "content-hash",
      runtime_gate_status: "ready",
      installed_at: "2026-07-15T00:00:00Z",
    }]);
    vi.spyOn(window, "confirm").mockReturnValue(true);
    renderPanel();

    fireEvent.click(await screen.findByRole("button", { name: "Details" }));
    fireEvent.click(await screen.findByRole("button", { name: "Delete version" }));

    await waitFor(() => expect(serviceMocks.deleteVersion).toHaveBeenCalledWith({
      packageId: "io.github.util6.codex-session",
      version: "1.0.0",
      confirmed: true,
    }));
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
    ahead_of_release: false,
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
    ahead_of_release: false,
    runtime_ready: true,
    status: "update_available",
    display_install_path: "~/conversation-adapters/codex",
    display_manifest_path: "~/conversation-adapters/codex/conversation-adapter.json",
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
    ahead_of_release: false,
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
