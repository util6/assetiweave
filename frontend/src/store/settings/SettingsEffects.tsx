import { useLayoutEffect } from "react";
import { applyThemeToElement } from "../../theme/cssVars";
import { resolveFontFamilyCss } from "./settingsSchema";
import { useAppSettings } from "./useAppSettings";

export function SettingsEffects(): null {
  const { settings } = useAppSettings();

  useLayoutEffect(() => {
    document.documentElement.dataset.density = settings.density;
    document.documentElement.style.setProperty(
      "--app-font-family",
      resolveFontFamilyCss(settings.typography.interfaceFontFamily, "sans"),
    );
    document.documentElement.style.setProperty(
      "--app-content-font-family",
      resolveFontFamilyCss(settings.typography.contentFontFamily, "sans"),
    );
    document.documentElement.style.setProperty(
      "--app-code-font-family",
      resolveFontFamilyCss(settings.typography.codeFontFamily, "mono"),
    );
    document.documentElement.style.setProperty(
      "--app-base-font-size",
      `${settings.typography.baseFontSize}px`,
    );
    document.documentElement.style.setProperty(
      "--app-content-font-size",
      `${settings.typography.contentFontSize}px`,
    );
    document.documentElement.style.setProperty(
      "--app-code-font-size",
      `${settings.typography.codeFontSize}px`,
    );
    applyThemeToElement(document.documentElement, settings.theme);
  }, [settings.density, settings.theme, settings.typography]);

  return null;
}
