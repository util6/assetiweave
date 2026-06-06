import type { ThemeDefinition, ThemeId } from "./schema";
import { getTheme } from "./themes";

export function themeCssVars(theme: ThemeDefinition): Record<string, string> {
  const { button, component, control, effect, navigation, palette, status, surface, switch: switchTokens } = theme.tokens;

  return {
    "--color-background": palette.background,
    "--color-surface": palette.surface,
    "--color-surface-lowest": palette.surfaceLowest,
    "--color-surface-low": palette.surfaceLow,
    "--color-surface-card": palette.surfaceCard,
    "--color-surface-high": palette.surfaceHigh,
    "--color-surface-highest": palette.surfaceHighest,
    "--color-border": palette.border,
    "--color-outline": palette.outline,
    "--color-outline-variant": palette.outlineVariant,
    "--color-on-surface": palette.onSurface,
    "--color-on-surface-variant": palette.onSurfaceVariant,
    "--color-primary": palette.primary,
    "--color-primary-strong": palette.primaryStrong,
    "--color-status-update": status.update,
    "--color-status-create": status.create,
    "--color-status-remove": status.remove,
    "--color-status-conflict": status.conflict,
    "--color-grid-line": palette.gridLine,
    "--theme-page-glow": surface.pageGlow,
    "--theme-page-sheen": surface.pageSheen,
    "--theme-grid-opacity": effect.gridOpacity,
    "--theme-inset-highlight": effect.insetHighlight,
    "--theme-panel-shadow": effect.panelShadow,
    "--theme-glow": effect.glow,
    "--theme-focus-ring": effect.focusRing,
    "--theme-scrim": effect.scrim,
    "--theme-glass-opacity": effect.glassOpacity,
    "--theme-hover-lift": effect.hoverLift === "raised" ? "-2px" : effect.hoverLift === "subtle" ? "-1px" : "0",
    "--theme-card-bg": surface.cardBg,
    "--theme-card-border": surface.cardBorder,
    "--theme-card-header": surface.cardHeader,
    "--theme-control-bg": control.bg,
    "--theme-control-hover": control.hover,
    "--theme-control-border": control.border,
    "--theme-control-fg": control.fg,
    "--theme-button-primary-bg": button.primaryBg,
    "--theme-button-primary-hover": button.primaryHover,
    "--theme-button-primary-fg": button.primaryFg,
    "--theme-nav-bg": navigation.bg,
    "--theme-nav-hover": navigation.hover,
    "--theme-nav-active": navigation.active,
    "--theme-nav-active-fg": navigation.activeFg,
    "--theme-nav-active-border": navigation.activeBorder,
    "--theme-nav-indicator": navigation.indicator,
    "--theme-subnav-bg": surface.subnavBg,
    "--theme-toolbar-bg": surface.toolbarBg,
    "--theme-switch-bg": switchTokens.bg,
    "--theme-switch-thumb": switchTokens.thumb,
    "--theme-switch-checked": switchTokens.checked,
    "--theme-switch-checked-thumb": switchTokens.checkedThumb,
    "--theme-shadow-card": component.cardShadow,
    "--theme-shadow-panel": component.panelShadow,
    "--theme-shadow-toolbar": component.toolbarShadow,
    "--theme-shadow-dialog": component.dialogShadow,
    "--theme-shadow-control-inset": component.controlInsetShadow,
    "--theme-shadow-active": component.activeShadow,
  };
}

export function applyThemeToElement(element: HTMLElement, themeId: ThemeId) {
  const theme = getTheme(themeId);
  element.dataset.theme = theme.id;
  element.style.colorScheme = theme.mode;

  for (const [name, value] of Object.entries(themeCssVars(theme))) {
    element.style.setProperty(name, value);
  }
}
