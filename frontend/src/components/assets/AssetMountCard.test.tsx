import { renderToStaticMarkup } from "react-dom/server";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { I18nProvider } from "../../i18n/I18nProvider";
import type { AppShortcut, Asset, AssetMountStatus, TargetProfile } from "../../types";
import { AssetMountCard } from "./AssetMountCard";

describe("AssetMountCard", () => {
  beforeEach(() => {
    vi.stubGlobal("localStorage", {
      getItem: () => "zh",
      setItem: vi.fn(),
    });
    vi.stubGlobal("navigator", { language: "zh-CN" });
  });

  it("uses the app accent color for the mounted confirmation state", () => {
    const html = renderToStaticMarkup(
      <I18nProvider>
        <AssetMountCard
          asset={skillAsset}
          mountStatus={mountedStatus}
          onToggle={vi.fn()}
          profile={profile}
          shortcut={shortcut}
        />
      </I18nProvider>,
    );

    expect(html).toContain("#c15f24");
    expect(html).not.toContain("status-create");
  });
});

const skillAsset: Asset = {
  id: "skill-a",
  source_id: "source-a",
  name: "skill-a",
  kind: "skill",
  format: "directory",
  relative_path: "skill-a",
  absolute_path: "/tmp/source-a/skill-a",
  entry_file: null,
  description: null,
  content_hash: null,
  discovered_at: "2026-01-01T00:00:00Z",
  updated_at: "2026-01-01T00:00:00Z",
};

const mountedStatus: AssetMountStatus = {
  asset_id: skillAsset.id,
  profile_id: "codex",
  target_dir: "/tmp/codex/skills",
  target_path: "/tmp/codex/skills/skill-a",
  state: "mounted",
  linked_source: skillAsset.absolute_path,
};

const profile: TargetProfile = {
  id: "codex",
  name: "Codex",
  app_kind: "codex",
  target_paths: ["/tmp/codex/skills"],
  supported_kinds: ["skill"],
  deployment_strategy: "symlink_to_source",
  enabled: true,
  include: {
    kinds: [],
    tags: [],
    groups: [],
    sources: [],
    path_patterns: [],
  },
  exclude: {
    kinds: [],
    tags: [],
    groups: [],
    sources: [],
    path_patterns: [],
  },
  safety: {
    allow_remove: true,
    allow_overwrite: false,
  },
};

const shortcut: AppShortcut = {
  profileId: profile.id,
  profileName: profile.name,
  appKind: profile.app_kind,
  displayIcon: "C",
  accentColor: "#c15f24",
  enabled: true,
};
