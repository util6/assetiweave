import type { Source } from "../types";

const MANAGED_SKILL_SOURCE_IDS = new Set([
  "assetiweave-library-skills",
  "assetiweave-system-skills",
]);

export function isManagedSkillSource(source: Source) {
  return MANAGED_SKILL_SOURCE_IDS.has(source.id)
    || source.source_origin === "assetiweave_library"
    || source.source_origin === "assetiweave_system";
}
