/* @vitest-environment jsdom */

import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { I18nProvider } from "../../i18n/I18nProvider";
import type { ConversationAdapterPackageCatalogEntry } from "../../services/conversations";
import { ConversationScriptResourceRow } from "./ConversationScriptResourceRow";

function makeEntry(
  overrides: Partial<ConversationAdapterPackageCatalogEntry> = {},
): ConversationAdapterPackageCatalogEntry {
  return {
    item: {
      id: "io.example.pkg",
      name: "Example Parser",
      version: "1.2.0",
      record_kind: "session",
      provider: "local_directory",
      adapter_id: "example",
      tags: [],
      manifest_file: "conversation-adapter.json",
      source: { type: "local_directory", url: "/tmp/example" },
    },
    installed: true,
    update_available: false,
    ahead_of_release: false,
    runtime_ready: true,
    status: "installed",
    display_manifest_path: "~/.assetiweave/example/conversation-adapter.json",
    installed_package: {
      package_id: "io.example.pkg",
      adapter_id: "example",
      name: "Example Parser",
      version: "1.1.0",
      record_kind: "session",
      install_dir: "/tmp/example/versions/1.1.0",
      manifest_path: "/tmp/example/conversation-adapter-package.json",
      adapter_manifest_path: "/tmp/example/conversation-adapter.json",
      runtime_protocol: "stdio-ndjson-v1",
      runtime_ready: true,
      origin: "local_directory",
      update_policy: "manual",
      latest_version: null,
      runtime_gate_status: "ready",
      installed_content_hash: "content-hash",
      trusted_package_hash: "trusted-hash",
      created_at: "2026-07-15T00:00:00Z",
      updated_at: "2026-07-15T00:00:00Z",
    },
    ...overrides,
  };
}

function renderRow(
  entry: ConversationAdapterPackageCatalogEntry,
  handlers: {
    onInstall?: () => void;
    onInspect?: () => void;
    onLifecycle?: () => void;
  } = {},
) {
  return render(
    <I18nProvider>
      <ConversationScriptResourceRow
        changeDisabled={false}
        disabled={false}
        entry={entry}
        onInstall={handlers.onInstall ?? vi.fn()}
        onInspect={handlers.onInspect ?? vi.fn()}
        onLifecycle={handlers.onLifecycle ?? vi.fn()}
      />
    </I18nProvider>,
  );
}

describe("ConversationScriptResourceRow", () => {
  afterEach(() => {
    cleanup();
  });

  it("renders an update action for a local directory package with a pending candidate", () => {
    const onInstall = vi.fn();
    renderRow(
      makeEntry({ update_available: true, status: "update_available" }),
      { onInstall },
    );

    fireEvent.click(screen.getByRole("button", { name: "Update" }));
    expect(onInstall).toHaveBeenCalledTimes(1);
    expect(
      screen.getByText("~/.assetiweave/example/conversation-adapter.json"),
    ).toBeTruthy();
  });

  it("renders a repair action for a non-managed package in a broken state", () => {
    renderRow(makeEntry({ status: "hash_mismatch" }));

    expect(screen.getByRole("button", { name: "Repair" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "Uninstall" })).toBeTruthy();
    expect(
      screen.queryByRole("button", { name: "Manage / delete" }),
    ).toBeNull();
  });

  it("offers register for a disabled built-in runtime without lifecycle actions", () => {
    renderRow(
      makeEntry({
        status: "uninstalled",
        runtime_ready: false,
        installed_package: null,
        installed_adapter: {
          id: "example",
          name: "Built-in Example",
          kind: "external",
          version: "1.0.0",
          enabled: false,
          trust_state: "built_in",
          capabilities: [],
          input_kinds: [],
          created_at: "2026-07-15T00:00:00Z",
          updated_at: "2026-07-15T00:00:00Z",
        },
      }),
    );

    expect(screen.getByRole("button", { name: "Register" })).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Uninstall" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Update" })).toBeNull();
  });

  it("renders no primary action for an up-to-date external package", () => {
    renderRow(makeEntry());

    expect(screen.getByRole("button", { name: "Details" })).toBeTruthy();
    expect(screen.queryByRole("button", { name: "Update" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Repair" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Register" })).toBeNull();
  });
});
