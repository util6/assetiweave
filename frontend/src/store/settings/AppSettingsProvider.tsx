import type { ReactNode } from "react";

export { useAppSettings, type AppSettingsContextValue } from "./useAppSettings";
export { SettingsEffects } from "./SettingsEffects";
export * from "./settingsSchema";

/**
 * @deprecated AppSettingsProvider is no longer needed; settings are managed via TanStack Query and SettingsEffects.
 */
export function AppSettingsProvider({ children }: { children: ReactNode }) {
  return <>{children}</>;
}
