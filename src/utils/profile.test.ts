import { describe, expect, it } from "vitest";
import type { TargetProfile } from "../types";
import { buildTargetProfileInput, deriveProfileId, hasProfileIdConflict, targetProfileFromInput } from "./profile";

describe("profile helpers", () => {
  it("derives stable profile ids from app names", () => {
    expect(deriveProfileId("Team App")).toBe("team-app");
    expect(deriveProfileId("  Codex++ Skills  ")).toBe("codex-skills");
  });

  it("detects duplicate profile ids except for the edited profile", () => {
    const profiles = [profile("codex"), profile("team-app")];

    expect(hasProfileIdConflict("team-app", profiles)).toBe(true);
    expect(hasProfileIdConflict("team-app", profiles, "team-app")).toBe(false);
  });

  it("builds skill-only target profile input from app form values", () => {
    expect(
      targetProfileFromInput(
        buildTargetProfileInput({
          accentColor: "#8c909f",
          appKind: "custom",
          displayIcon: "T",
          enabled: true,
          name: " Team App ",
          shortcutEnabled: true,
          targetPath: " ~/team-app/skills ",
        }),
      ),
    ).toMatchObject({
      app_kind: "custom",
      id: "team-app",
      include: { kinds: ["skill"] },
      name: "Team App",
      supported_kinds: ["skill"],
      target_paths: ["~/team-app/skills"],
    });
  });
});

function profile(id: string): TargetProfile {
  return {
    app_kind: "custom",
    deployment_strategy: "symlink_to_source",
    enabled: true,
    exclude: { groups: [], kinds: ["unclassified"], path_patterns: [], sources: [], tags: [] },
    id,
    include: { groups: [], kinds: ["skill"], path_patterns: [], sources: [], tags: [] },
    name: id,
    safety: { allow_overwrite: false, allow_remove: false },
    supported_kinds: ["skill"],
    target_paths: [`~/${id}/skills`],
  };
}
