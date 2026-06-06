import type { Source } from "../types";

export function isDirectMountBlockedSource(source: Source | null | undefined) {
  return source?.source_origin === "app_target" || source?.source_origin === "app_local";
}
