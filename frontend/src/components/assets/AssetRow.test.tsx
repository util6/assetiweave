/* @vitest-environment jsdom */

import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
} from "@testing-library/react";
import { renderToStaticMarkup } from "react-dom/server";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { I18nProvider } from "../../i18n/I18nProvider";
import type { Asset, Source } from "../../types";
import { AssetRow } from "./AssetRow";

describe("AssetRow", () => {
  beforeEach(() => {
    vi.stubGlobal("localStorage", {
      getItem: () => "zh",
      setItem: vi.fn(),
    });
    vi.stubGlobal("navigator", { language: "zh-CN" });
  });

  afterEach(() => {
    cleanup();
    vi.useRealTimers();
    vi.unstubAllGlobals();
  });

  it("shows the precise repository remote instead of the source id", () => {
    const html = renderToStaticMarkup(
      <I18nProvider>
        <AssetRow
          appShortcuts={[]}
          asset={asset}
          expanded={false}
          mountStatuses={[]}
          onRevealPath={vi.fn()}
          onToggleExpanded={vi.fn()}
          onToggleMount={vi.fn()}
          profiles={[]}
          source={source}
        />
      </I18nProvider>,
    );

    expect(html).toContain("https://github.com/anthropics/skills.git");
    expect(html).not.toContain("hash-like-source-id");
    expect(html).not.toContain("backdrop-blur");
  });

  it("renders the repository source as a browser link", () => {
    const html = renderToStaticMarkup(
      <I18nProvider>
        <AssetRow
          appShortcuts={[]}
          asset={asset}
          expanded={false}
          mountStatuses={[]}
          onRevealPath={vi.fn()}
          onToggleExpanded={vi.fn()}
          onToggleMount={vi.fn()}
          profiles={[]}
          source={source}
        />
      </I18nProvider>,
    );

    expect(html).toContain(
      'href="https://github.com/anthropics/skills/tree/main/packages/claude-skills"',
    );
  });

  it("exposes Reveal in Finder via aria-label without native title and shows TruncatedText tooltip only when path overflows", () => {
    vi.useFakeTimers();
    render(
      <I18nProvider>
        <AssetRow
          appShortcuts={[]}
          asset={asset}
          expanded={false}
          mountStatuses={[]}
          onRevealPath={vi.fn()}
          onToggleExpanded={vi.fn()}
          onToggleMount={vi.fn()}
          profiles={[]}
          source={source}
        />
      </I18nProvider>,
    );

    const revealButton = screen.getByRole("button", {
      name: "在文件管理器中显示",
    });
    expect(revealButton.hasAttribute("title")).toBe(false);
    expect(revealButton.className).not.toContain("asset-description");

    const pathText = screen.getByText("~/fork-code/skills-fork/claude-skills");
    expect(pathText.className).toContain("truncate");

    Object.defineProperty(pathText, "clientWidth", {
      configurable: true,
      get: () => 120,
    });
    Object.defineProperty(pathText, "scrollWidth", {
      configurable: true,
      get: () => 280,
    });
    Object.defineProperty(pathText, "clientHeight", {
      configurable: true,
      get: () => 20,
    });
    Object.defineProperty(pathText, "scrollHeight", {
      configurable: true,
      get: () => 20,
    });

    fireEvent.mouseEnter(pathText);
    act(() => {
      vi.advanceTimersByTime(300);
    });

    const tooltip = screen.getByRole("tooltip");
    expect(tooltip.textContent).toBe("~/fork-code/skills-fork/claude-skills");
  });
});

const asset: Asset = {
  id: "asset-id",
  source_id: "hash-like-source-id",
  name: "claude-skills",
  kind: "skill",
  format: "directory",
  relative_path: "claude-skills",
  stored_path: "/Users/util6/fork-code/skills-fork/claude-skills",
  entry_file: null,
  description: null,
  content_hash: null,
  discovered_at: "2026-01-01T00:00:00Z",
  updated_at: "2026-01-01T00:00:00Z",
  repository: {
    root_path: "/Users/util6/fork-code/skills-fork/claude-skills",
    remote_url: "https://github.com/anthropics/skills.git",
    web_url:
      "https://github.com/anthropics/skills/tree/main/packages/claude-skills",
  },
};

const source: Source = {
  id: "hash-like-source-id",
  name: "skills-fork",
  kind: "local",
  root_path: "/Users/util6/fork-code/skills-fork",
  scanner_kind: "skill",
  source_origin: "local_folder",
  repo_root: null,
  scan_root: "",
  origin_app_kind: null,
  include_globs: ["**/SKILL.md"],
  exclude_globs: [],
  default_kind: "skill",
  enabled: true,
  priority: 0,
  last_scanned_at: null,
  last_scan_status: null,
};
