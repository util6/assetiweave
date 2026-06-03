import type { TranslationKey } from "../i18n/messages";

export type ThemeId = "midnight" | "sunlight" | "violetDark" | "violetLight";
export type ThemeMode = "dark" | "light";
export type ThemeHoverLift = "none" | "subtle" | "raised";
export type RgbString = `${number} ${number} ${number}`;

export interface ThemeDefinition {
  id: ThemeId;
  labelKey: TranslationKey;
  mode: ThemeMode;
  swatches: string[];
  tokens: ThemeTokens;
}

export interface ThemeTokens {
  palette: {
    background: RgbString;
    surface: RgbString;
    surfaceLowest: RgbString;
    surfaceLow: RgbString;
    surfaceCard: RgbString;
    surfaceHigh: RgbString;
    surfaceHighest: RgbString;
    border: RgbString;
    outline: RgbString;
    outlineVariant: RgbString;
    onSurface: RgbString;
    onSurfaceVariant: RgbString;
    primary: RgbString;
    primaryStrong: RgbString;
    gridLine: RgbString;
  };
  status: {
    update: RgbString;
    create: RgbString;
    remove: RgbString;
    conflict: RgbString;
  };
  surface: {
    pageGlow: RgbString;
    pageSheen: RgbString;
    cardBg: RgbString;
    cardBorder: RgbString;
    cardHeader: RgbString;
    subnavBg: RgbString;
    toolbarBg: RgbString;
  };
  control: {
    bg: RgbString;
    hover: RgbString;
    border: RgbString;
    fg: RgbString;
  };
  navigation: {
    bg: RgbString;
    hover: RgbString;
    active: RgbString;
    activeFg: RgbString;
    activeBorder: RgbString;
    indicator: RgbString;
  };
  button: {
    primaryBg: RgbString;
    primaryHover: RgbString;
    primaryFg: RgbString;
  };
  switch: {
    bg: RgbString;
    thumb: RgbString;
    checked: RgbString;
    checkedThumb: RgbString;
  };
  effect: {
    gridOpacity: string;
    insetHighlight: RgbString;
    panelShadow: RgbString;
    glow: RgbString;
    focusRing: RgbString;
    scrim: RgbString;
    glassOpacity: string;
    hoverLift: ThemeHoverLift;
  };
  component: {
    cardShadow: string;
    panelShadow: string;
    toolbarShadow: string;
    dialogShadow: string;
    controlInsetShadow: string;
    activeShadow: string;
  };
}

export function defineTheme(theme: ThemeDefinition): ThemeDefinition {
  return theme;
}
