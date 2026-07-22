import { describe, expect, it } from "vitest";
import type { Source } from "../types";
import { isManagedSkillSource } from "./sourcePolicy";

describe("isManagedSkillSource", () => {
  it.each([
    ["assetiweave-library-skills", "assetiweave_library"],
    ["assetiweave-system-skills", "assetiweave_system"],
  ] as const)("protects %s", (id, sourceOrigin) => {
    expect(isManagedSkillSource(source(id, sourceOrigin))).toBe(true);
  });

  it("allows normal sources to be edited", () => {
    expect(isManagedSkillSource(source("local-skills", "local_folder"))).toBe(false);
  });
});

function source(id: string, sourceOrigin: Source["source_origin"]): Source {
  return {
    id,
    name: id,
    kind: "local",
    root_path: "/tmp/skills",
    scanner_kind: "skill",
    source_origin: sourceOrigin,
    repo_root: null,
    scan_root: "",
    origin_app_kind: null,
    include_globs: ["**/SKILL.md"],
    exclude_globs: [],
    default_kind: "skill",
    enabled: true,
    priority: 0,
    last_scanned_at: null,
    last_scan_status: "pending",
  };
}
