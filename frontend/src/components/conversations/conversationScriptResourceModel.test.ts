import { describe, expect, it } from "vitest";
import type { ConversationAdapterPackageCatalogEntry } from "../../services/conversations";
import {
  actionLabelKey,
  countUpdateOrRepairEntries,
  isUpdateOrRepairEntry,
  manifestPathForEntry,
  manifestPathFromInstallResult,
  packageActionForEntry,
  packageLifecycleAction,
  packageTrustSummary,
  REPAIR_STATUSES,
  resolvePackageChangeVersion,
  statusBadgeTone,
  statusLabelKey,
} from "./conversationScriptResourceModel";

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
    install_path: "/tmp/example",
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

describe("packageActionForEntry", () => {
  it("offers update for a local directory package with a pending candidate", () => {
    expect(
      packageActionForEntry(
        makeEntry({ update_available: true, status: "update_available" }),
      ),
    ).toBe("update");
  });

  it("offers update for a managed release package", () => {
    const entry = makeEntry({
      update_available: true,
      status: "update_available",
    });
    entry.installed_package = {
      ...entry.installed_package!,
      origin: "managed_release",
    };
    expect(packageActionForEntry(entry)).toBe("update");
  });

  it("offers update for a built-in package flagged by the registry", () => {
    const entry = makeEntry({
      update_available: true,
      status: "update_available",
    });
    entry.installed_package = {
      ...entry.installed_package!,
      origin: "built_in",
    };
    expect(packageActionForEntry(entry)).toBe("update");
  });

  it("does not offer any primary action for an up-to-date external package", () => {
    expect(packageActionForEntry(makeEntry())).toBeNull();
    expect(
      packageActionForEntry(makeEntry({ status: "local_registered" })),
    ).toBeNull();
  });

  it("does not offer an update when the installed package is ahead", () => {
    expect(
      packageActionForEntry(
        makeEntry({ status: "ahead_of_release", ahead_of_release: true }),
      ),
    ).toBeNull();
  });

  it.each(REPAIR_STATUSES)(
    "offers repair for a local package in %s state",
    (status) => {
      const entry = makeEntry({ status });
      expect(entry.installed_package?.origin).toBe("local_directory");
      expect(packageActionForEntry(entry)).toBe("repair");
    },
  );

  it("offers register for an uninstalled package", () => {
    expect(
      packageActionForEntry(
        makeEntry({ status: "uninstalled", runtime_ready: false }),
      ),
    ).toBe("install");
  });

  it("offers install for a package that is not installed", () => {
    expect(
      packageActionForEntry(
        makeEntry({
          installed: false,
          status: "not_installed",
          runtime_ready: false,
          installed_package: null,
        }),
      ),
    ).toBe("install");
  });
});

describe("isUpdateOrRepairEntry", () => {
  it("counts updates and repairs but not healthy or discover entries", () => {
    const entries = [
      makeEntry({ update_available: true, status: "update_available" }),
      makeEntry({ status: "hash_mismatch" }),
      makeEntry({ status: "verification_failed" }),
      makeEntry({ status: "installed" }),
      makeEntry({ status: "ahead_of_release", ahead_of_release: true }),
      makeEntry({ installed: false, status: "not_installed" }),
    ];

    expect(entries.filter(isUpdateOrRepairEntry)).toHaveLength(3);
    expect(countUpdateOrRepairEntries(entries)).toBe(3);
  });
});

describe("packageLifecycleAction", () => {
  it("uninstalls managed release packages", () => {
    const entry = makeEntry();
    entry.installed_package = {
      ...entry.installed_package!,
      origin: "managed_release",
    };
    expect(packageLifecycleAction(entry)).toBe("uninstall");
  });

  it("unregisters external packages", () => {
    expect(packageLifecycleAction(makeEntry())).toBe("unregister");
  });

  it("hides the lifecycle action when nothing is installed or the status is uninstalled", () => {
    expect(packageLifecycleAction(makeEntry({ installed: false }))).toBeNull();
    expect(
      packageLifecycleAction(makeEntry({ status: "uninstalled" })),
    ).toBeNull();
  });

  it("offers unregister for a broken local package but none for a broken managed release", () => {
    // #199: an unreadable local package keeps an actionable row so the user can
    // unregister it and re-register a valid source directory.
    expect(
      packageLifecycleAction(makeEntry({ runtime_ready: false })),
    ).toBe("unregister");
    const managedBroken = makeEntry({ runtime_ready: false });
    managedBroken.installed_package = {
      ...managedBroken.installed_package!,
      origin: "managed_release",
    };
    expect(packageLifecycleAction(managedBroken)).toBeNull();
  });
});

describe("packageTrustSummary", () => {
  it("exposes trusted version, managed directory version and hashes", () => {
    expect(packageTrustSummary(makeEntry())).toEqual({
      trustedVersion: "1.1.0",
      managedDirectoryVersion: "1.2.0",
      contentHash: "content-hash",
      trustedHash: "trusted-hash",
    });
  });

  it("falls back to the installed adapter and null for missing values", () => {
    const entry = makeEntry({
      installed_package: null,
      installed_adapter: {
        id: "example",
        name: "Example",
        kind: "external",
        version: "1.0.0",
        enabled: true,
        content_hash: "adapter-content-hash",
        trusted_hash: "adapter-trusted-hash",
        trust_state: "trusted",
        capabilities: [],
        input_kinds: [],
        created_at: "2026-07-15T00:00:00Z",
        updated_at: "2026-07-15T00:00:00Z",
      },
    });

    expect(packageTrustSummary(entry)).toEqual({
      trustedVersion: "1.0.0",
      managedDirectoryVersion: "1.2.0",
      contentHash: "adapter-content-hash",
      trustedHash: "adapter-trusted-hash",
    });
    expect(
      packageTrustSummary(
        makeEntry({
          installed_package: null,
          installed_adapter: null,
          item: { ...makeEntry().item, version: "" },
        }),
      ),
    ).toEqual({
      trustedVersion: null,
      managedDirectoryVersion: null,
      contentHash: null,
      trustedHash: null,
    });
  });
});

describe("resolvePackageChangeVersion", () => {
  it("returns the selected version only when it is a real release of the same entry", () => {
    expect(
      resolvePackageChangeVersion({
        entryId: "io.example.pkg",
        detailEntryId: "io.example.pkg",
        releaseVersions: ["1.2.0"],
        selectedVersion: "1.2.0",
      }),
    ).toBe("1.2.0");
  });

  it("never forwards a version for local packages without release history", () => {
    expect(
      resolvePackageChangeVersion({
        entryId: "io.example.pkg",
        detailEntryId: "io.example.pkg",
        releaseVersions: [],
        selectedVersion: "1.2.0",
      }),
    ).toBeUndefined();
    expect(
      resolvePackageChangeVersion({
        entryId: "io.example.pkg",
        detailEntryId: "io.example.pkg",
        releaseVersions: ["1.2.0"],
        selectedVersion: "9.9.9",
      }),
    ).toBeUndefined();
  });

  it("ignores the selection when the detail dialog belongs to another entry", () => {
    expect(
      resolvePackageChangeVersion({
        entryId: "io.example.pkg",
        detailEntryId: "other.pkg",
        releaseVersions: ["1.2.0"],
        selectedVersion: "1.2.0",
      }),
    ).toBeUndefined();
    expect(
      resolvePackageChangeVersion({
        entryId: "io.example.pkg",
        detailEntryId: null,
        releaseVersions: ["1.2.0"],
        selectedVersion: "1.2.0",
      }),
    ).toBeUndefined();
  });
});

describe("labels and paths", () => {
  it("maps repair-oriented statuses to their copy and tone", () => {
    expect(statusLabelKey("hash_mismatch")).toBe(
      "conversation.scriptMarket.hashMismatch",
    );
    expect(statusLabelKey("local_registered")).toBe(
      "conversation.scriptMarket.localRegistered",
    );
    expect(statusBadgeTone("hash_mismatch")).toBe("remove");
    expect(statusBadgeTone("update_available")).toBe("conflict");
    expect(statusBadgeTone("local_registered")).toBe("create");
    expect(statusBadgeTone("not_installed")).toBe("primary");
    expect(actionLabelKey("repair")).toBe("conversation.scriptMarket.repair");
    expect(actionLabelKey("unregister")).toBe(
      "conversation.scriptMarket.uninstall",
    );
  });

  it("resolves manifest paths from the installed package, then the install directory", () => {
    expect(manifestPathForEntry(makeEntry())).toBe(
      "/tmp/example/conversation-adapter.json",
    );
    expect(
      manifestPathForEntry(
        makeEntry({
          installed_package: null,
          installed_adapter: null,
          item: { ...makeEntry().item, manifest_file: "adapter.json" },
        }),
      ),
    ).toBe("/tmp/example/adapter.json");
    expect(
      manifestPathForEntry(
        makeEntry({
          installed_package: null,
          installed_adapter: null,
          install_path: null,
        }),
      ),
    ).toBeNull();
  });

  it("extracts a non-empty manifest path from install results", () => {
    expect(
      manifestPathFromInstallResult({ manifest_path: "/tmp/a.json" }),
    ).toBe("/tmp/a.json");
    expect(manifestPathFromInstallResult({ manifest_path: "  " })).toBeNull();
    expect(manifestPathFromInstallResult(null)).toBeNull();
    expect(manifestPathFromInstallResult("plain")).toBeNull();
  });
});
