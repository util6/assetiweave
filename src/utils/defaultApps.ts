export const DEFAULT_APP_PROFILE_IDS = [
  "claude",
  "codex",
  "gemini",
  "opencode",
  "cursor",
  "antigravity",
  "openclaw",
] as const;

export function isDefaultAppProfileId(profileId: string) {
  return DEFAULT_APP_PROFILE_IDS.includes(profileId as (typeof DEFAULT_APP_PROFILE_IDS)[number]);
}
