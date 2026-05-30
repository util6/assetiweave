import { describe, expect, it } from "vitest";
import {
  buildImportSourceInput,
  DEFAULT_SKILL_EXCLUDE_GLOBS,
  DEFAULT_SKILL_INCLUDE_GLOBS,
  deriveSourceName,
  validateSourceImportForm,
} from "./sourceImport";

describe("source import helpers", () => {
  it("builds a skill source input with safe scan defaults", () => {
    expect(
      buildImportSourceInput({
        enabled: true,
        excludeGlobsText: "",
        includeGlobsText: "",
        name: "  Team Skills  ",
        priority: "30",
        rootPath: "  ~/code/team-skills  ",
      }),
    ).toEqual({
      default_kind: "skill",
      enabled: true,
      exclude_globs: DEFAULT_SKILL_EXCLUDE_GLOBS,
      include_globs: DEFAULT_SKILL_INCLUDE_GLOBS,
      kind: "import",
      name: "Team Skills",
      origin_app_kind: null,
      priority: 30,
      repo_root: null,
      root_path: "~/code/team-skills",
      scan_root: "",
      scanner_kind: "skill",
      source_origin: "local_folder",
    });
  });

  it("splits custom glob lines and derives a source name from the path", () => {
    expect(
      buildImportSourceInput({
        enabled: false,
        excludeGlobsText: "\n**/.git/**\n **/vendor/** ",
        includeGlobsText: "**/SKILL.md\npackages/*/SKILL.md\n",
        name: "",
        priority: "7",
        rootPath: "~/code-space/util6-agents/",
      }),
    ).toMatchObject({
      enabled: false,
      exclude_globs: ["**/.git/**", "**/vendor/**"],
      include_globs: ["**/SKILL.md", "packages/*/SKILL.md"],
      name: "util6-agents",
      priority: 7,
    });
  });

  it("reports missing path and invalid priority before submit", () => {
    expect(
      validateSourceImportForm({
        enabled: true,
        excludeGlobsText: "",
        includeGlobsText: "",
        name: "",
        priority: "1.5",
        rootPath: "",
      }),
    ).toEqual({
      priority: "invalid",
      rootPath: "required",
    });
  });

  it("derives readable names across common path formats", () => {
    expect(deriveSourceName("~/code/skills/")).toBe("skills");
    expect(deriveSourceName("/Users/util6/code-space/util6-agents")).toBe("util6-agents");
    expect(deriveSourceName("C:\\Users\\util6\\skills")).toBe("skills");
  });
});
