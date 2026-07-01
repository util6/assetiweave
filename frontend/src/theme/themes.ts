import { defineTheme, type ThemeDefinition, type ThemeId } from "./schema";

export const DEFAULT_ENTITY_ACCENT_HEX = "#5f7fb8";
export const DEFAULT_GROUP_COLOR_HEX = "#2f9d78";

const themes = [
  defineTheme({
    id: "promptStudio",
    labelKey: "settings.theme.promptStudio",
    mode: "dark",
    swatches: ["#120b25", "#17141f", "#27c7f4", "#d946b8"],
    tokens: {
      palette: {
        background: "8 7 17",
        surface: "17 14 28",
        surfaceLowest: "5 5 12",
        surfaceLow: "13 10 24",
        surfaceCard: "24 22 33",
        surfaceHigh: "33 31 45",
        surfaceHighest: "45 42 62",
        border: "45 42 64",
        outline: "151 146 173",
        outlineVariant: "71 67 92",
        onSurface: "244 243 250",
        onSurfaceVariant: "197 193 214",
        primary: "211 243 255",
        primaryStrong: "39 199 244",
        gridLine: "160 150 196",
      },
      status: {
        update: "45 156 255",
        create: "47 211 156",
        remove: "217 70 184",
        conflict: "246 199 61",
      },
      surface: {
        pageGlow: "101 73 255",
        pageSheen: "217 70 184",
        cardBg: "24 22 33",
        cardBorder: "45 42 64",
        cardHeader: "18 16 27",
        subnavBg: "10 8 20",
        toolbarBg: "14 12 24",
      },
      control: {
        bg: "24 22 33",
        hover: "36 32 50",
        border: "55 52 75",
        fg: "197 193 214",
      },
      navigation: {
        bg: "10 8 20",
        hover: "27 23 39",
        active: "34 29 58",
        activeFg: "236 248 255",
        activeBorder: "39 199 244",
        indicator: "217 70 184",
      },
      button: {
        primaryBg: "45 156 255",
        primaryHover: "39 199 244",
        primaryFg: "255 255 255",
      },
      switch: {
        bg: "45 42 62",
        thumb: "197 193 214",
        checked: "47 211 156",
        checkedThumb: "255 255 255",
      },
      effect: {
        gridOpacity: "0.012",
        insetHighlight: "255 255 255",
        panelShadow: "2 1 8",
        glow: "39 199 244",
        focusRing: "45 156 255",
        scrim: "5 5 12",
        glassOpacity: "0.82",
        hoverLift: "subtle",
      },
      component: {
        cardShadow: "0 22px 60px rgb(var(--theme-panel-shadow) / 0.46)",
        panelShadow: "0 28px 80px rgb(var(--theme-panel-shadow) / 0.54)",
        toolbarShadow: "0 14px 36px rgb(var(--theme-panel-shadow) / 0.36)",
        dialogShadow: "0 32px 90px rgb(var(--theme-panel-shadow) / 0.62)",
        controlInsetShadow: "inset 0 1px 0 rgb(var(--theme-inset-highlight) / 0.2)",
        activeShadow: "0 14px 34px rgb(var(--theme-glow) / 0.26)",
      },
    },
  }),
  defineTheme({
    id: "midnight",
    labelKey: "settings.theme.midnight",
    mode: "dark",
    swatches: ["#101827", "#1d283a", "#5f8fd9", "#2f9d78"],
    tokens: {
      palette: {
        background: "12 18 29",
        surface: "21 29 43",
        surfaceLowest: "7 11 20",
        surfaceLow: "16 23 35",
        surfaceCard: "28 38 55",
        surfaceHigh: "35 46 64",
        surfaceHighest: "46 59 78",
        border: "65 80 102",
        outline: "144 154 173",
        outlineVariant: "84 98 121",
        onSurface: "227 234 246",
        onSurfaceVariant: "196 205 221",
        primary: "185 211 255",
        primaryStrong: "95 143 217",
        gridLine: "203 213 225",
      },
      status: {
        update: "70 164 213",
        create: "47 157 120",
        remove: "232 97 122",
        conflict: "226 162 77",
      },
      surface: {
        pageGlow: "95 143 217",
        pageSheen: "185 211 255",
        cardBg: "18 26 39",
        cardBorder: "65 80 102",
        cardHeader: "12 18 29",
        subnavBg: "14 21 32",
        toolbarBg: "18 26 39",
      },
      control: {
        bg: "35 46 64",
        hover: "47 61 82",
        border: "70 86 110",
        fg: "196 205 221",
      },
      navigation: {
        bg: "14 21 32",
        hover: "38 50 69",
        active: "44 59 82",
        activeFg: "185 211 255",
        activeBorder: "95 143 217",
        indicator: "185 211 255",
      },
      button: {
        primaryBg: "95 143 217",
        primaryHover: "79 127 202",
        primaryFg: "255 255 255",
      },
      switch: {
        bg: "47 61 82",
        thumb: "151 163 184",
        checked: "47 157 120",
        checkedThumb: "255 255 255",
      },
      effect: {
        gridOpacity: "0.018",
        insetHighlight: "255 255 255",
        panelShadow: "3 8 18",
        glow: "95 143 217",
        focusRing: "95 143 217",
        scrim: "3 8 18",
        glassOpacity: "0.74",
        hoverLift: "subtle",
      },
      component: {
        cardShadow: "0 18px 42px rgb(var(--theme-panel-shadow) / 0.22)",
        panelShadow: "0 20px 48px rgb(var(--theme-panel-shadow) / 0.26)",
        toolbarShadow: "0 12px 28px rgb(var(--theme-panel-shadow) / 0.18)",
        dialogShadow: "0 24px 70px rgb(var(--theme-panel-shadow) / 0.34)",
        controlInsetShadow: "inset 0 1px 0 rgb(var(--theme-inset-highlight) / 0.4)",
        activeShadow: "0 10px 24px rgb(var(--theme-panel-shadow) / 0.22)",
      },
    },
  }),
  defineTheme({
    id: "sunlight",
    labelKey: "settings.theme.sunlight",
    mode: "light",
    swatches: ["#fffaf0", "#ffffff", "#d89018", "#4f7fb8"],
    tokens: {
      palette: {
        background: "255 250 240",
        surface: "255 247 230",
        surfaceLowest: "255 253 247",
        surfaceLow: "255 245 221",
        surfaceCard: "255 255 255",
        surfaceHigh: "255 239 197",
        surfaceHighest: "249 220 144",
        border: "221 190 118",
        outline: "119 98 59",
        outlineVariant: "207 173 105",
        onSurface: "50 43 31",
        onSurfaceVariant: "91 77 52",
        primary: "109 78 32",
        primaryStrong: "216 144 24",
        gridLine: "166 129 70",
      },
      status: {
        update: "60 125 161",
        create: "47 145 96",
        remove: "210 69 77",
        conflict: "198 129 30",
      },
      surface: {
        pageGlow: "216 144 24",
        pageSheen: "79 127 184",
        cardBg: "255 255 255",
        cardBorder: "221 190 118",
        cardHeader: "255 247 230",
        subnavBg: "255 247 230",
        toolbarBg: "255 250 240",
      },
      control: {
        bg: "255 253 247",
        hover: "255 239 197",
        border: "221 190 118",
        fg: "91 77 52",
      },
      navigation: {
        bg: "255 253 247",
        hover: "255 239 197",
        active: "255 227 154",
        activeFg: "50 43 31",
        activeBorder: "216 144 24",
        indicator: "216 144 24",
      },
      button: {
        primaryBg: "216 144 24",
        primaryHover: "190 120 14",
        primaryFg: "35 27 12",
      },
      switch: {
        bg: "255 239 197",
        thumb: "119 98 59",
        checked: "216 144 24",
        checkedThumb: "255 255 255",
      },
      effect: {
        gridOpacity: "0.035",
        insetHighlight: "255 255 255",
        panelShadow: "128 95 48",
        glow: "216 144 24",
        focusRing: "79 127 184",
        scrim: "92 70 38",
        glassOpacity: "0.82",
        hoverLift: "subtle",
      },
      component: {
        cardShadow: "0 18px 38px rgb(var(--theme-panel-shadow) / 0.12)",
        panelShadow: "0 20px 42px rgb(var(--theme-panel-shadow) / 0.16)",
        toolbarShadow: "0 12px 24px rgb(var(--theme-panel-shadow) / 0.12)",
        dialogShadow: "0 24px 70px rgb(var(--theme-panel-shadow) / 0.2)",
        controlInsetShadow: "inset 0 1px 0 rgb(var(--theme-inset-highlight) / 0.72)",
        activeShadow: "0 10px 24px rgb(var(--theme-panel-shadow) / 0.14)",
      },
    },
  }),
  defineTheme({
    id: "violetDark",
    labelKey: "settings.theme.violetDark",
    mode: "dark",
    swatches: ["#0d0b14", "#1f1a2b", "#9d7bd8", "#e6d7ff"],
    tokens: {
      palette: {
        background: "13 11 20",
        surface: "24 20 34",
        surfaceLowest: "6 5 11",
        surfaceLow: "18 15 27",
        surfaceCard: "31 26 43",
        surfaceHigh: "43 36 58",
        surfaceHighest: "59 49 78",
        border: "82 68 107",
        outline: "163 149 187",
        outlineVariant: "101 84 130",
        onSurface: "240 235 248",
        onSurfaceVariant: "207 198 222",
        primary: "230 215 255",
        primaryStrong: "157 123 216",
        gridLine: "230 215 255",
      },
      status: {
        update: "133 144 230",
        create: "63 177 122",
        remove: "239 111 136",
        conflict: "222 155 75",
      },
      surface: {
        pageGlow: "157 123 216",
        pageSheen: "230 215 255",
        cardBg: "24 20 34",
        cardBorder: "82 68 107",
        cardHeader: "31 26 43",
        subnavBg: "18 15 27",
        toolbarBg: "24 20 34",
      },
      control: {
        bg: "43 36 58",
        hover: "59 49 78",
        border: "82 68 107",
        fg: "207 198 222",
      },
      navigation: {
        bg: "13 11 20",
        hover: "43 36 58",
        active: "59 49 78",
        activeFg: "230 215 255",
        activeBorder: "157 123 216",
        indicator: "230 215 255",
      },
      button: {
        primaryBg: "157 123 216",
        primaryHover: "138 99 201",
        primaryFg: "255 255 255",
      },
      switch: {
        bg: "59 49 78",
        thumb: "163 149 187",
        checked: "157 123 216",
        checkedThumb: "255 255 255",
      },
      effect: {
        gridOpacity: "0.02",
        insetHighlight: "255 255 255",
        panelShadow: "5 4 12",
        glow: "157 123 216",
        focusRing: "157 123 216",
        scrim: "5 4 12",
        glassOpacity: "0.76",
        hoverLift: "subtle",
      },
      component: {
        cardShadow: "0 18px 44px rgb(var(--theme-panel-shadow) / 0.28)",
        panelShadow: "0 22px 54px rgb(var(--theme-panel-shadow) / 0.34)",
        toolbarShadow: "0 12px 30px rgb(var(--theme-panel-shadow) / 0.22)",
        dialogShadow: "0 24px 76px rgb(var(--theme-panel-shadow) / 0.42)",
        controlInsetShadow: "inset 0 1px 0 rgb(var(--theme-inset-highlight) / 0.28)",
        activeShadow: "0 10px 28px rgb(var(--theme-glow) / 0.2)",
      },
    },
  }),
  defineTheme({
    id: "violetLight",
    labelKey: "settings.theme.violetLight",
    mode: "light",
    swatches: ["#fbf8ff", "#ffffff", "#7d5cc7", "#d9c8ff"],
    tokens: {
      palette: {
        background: "251 248 255",
        surface: "255 255 255",
        surfaceLowest: "248 244 255",
        surfaceLow: "251 247 255",
        surfaceCard: "255 255 255",
        surfaceHigh: "242 235 255",
        surfaceHighest: "225 214 248",
        border: "202 185 232",
        outline: "101 83 128",
        outlineVariant: "181 160 218",
        onSurface: "43 35 55",
        onSurfaceVariant: "82 68 103",
        primary: "91 66 143",
        primaryStrong: "125 92 199",
        gridLine: "125 92 199",
      },
      status: {
        update: "91 111 199",
        create: "47 145 96",
        remove: "214 69 101",
        conflict: "196 135 32",
      },
      surface: {
        pageGlow: "125 92 199",
        pageSheen: "217 200 255",
        cardBg: "255 255 255",
        cardBorder: "202 185 232",
        cardHeader: "251 247 255",
        subnavBg: "251 247 255",
        toolbarBg: "255 255 255",
      },
      control: {
        bg: "248 244 255",
        hover: "225 214 248",
        border: "202 185 232",
        fg: "82 68 103",
      },
      navigation: {
        bg: "255 255 255",
        hover: "242 235 255",
        active: "225 214 248",
        activeFg: "91 66 143",
        activeBorder: "125 92 199",
        indicator: "125 92 199",
      },
      button: {
        primaryBg: "125 92 199",
        primaryHover: "105 75 178",
        primaryFg: "255 255 255",
      },
      switch: {
        bg: "225 214 248",
        thumb: "101 83 128",
        checked: "125 92 199",
        checkedThumb: "255 255 255",
      },
      effect: {
        gridOpacity: "0.03",
        insetHighlight: "255 255 255",
        panelShadow: "86 62 135",
        glow: "125 92 199",
        focusRing: "125 92 199",
        scrim: "75 54 120",
        glassOpacity: "0.84",
        hoverLift: "subtle",
      },
      component: {
        cardShadow: "0 18px 40px rgb(var(--theme-panel-shadow) / 0.12)",
        panelShadow: "0 22px 46px rgb(var(--theme-panel-shadow) / 0.16)",
        toolbarShadow: "0 12px 24px rgb(var(--theme-panel-shadow) / 0.1)",
        dialogShadow: "0 24px 70px rgb(var(--theme-panel-shadow) / 0.22)",
        controlInsetShadow: "inset 0 1px 0 rgb(var(--theme-inset-highlight) / 0.76)",
        activeShadow: "0 10px 24px rgb(var(--theme-panel-shadow) / 0.14)",
      },
    },
  }),
] satisfies ThemeDefinition[];

export const themeRegistry: Record<ThemeId, ThemeDefinition> = Object.fromEntries(
  themes.map((theme) => [theme.id, theme]),
) as Record<ThemeId, ThemeDefinition>;

export const themeOptions = themes.map(({ id, labelKey, swatches }) => ({ id, labelKey, swatches }));

export function isThemeId(value: unknown): value is ThemeId {
  return typeof value === "string" && value in themeRegistry;
}

export function normalizeThemeId(value: unknown): ThemeId {
  if (isThemeId(value)) {
    return value;
  }

  if (value === "graphite") {
    return "violetDark";
  }

  if (value === "forest" || value === "ember") {
    return "sunlight";
  }

  return "promptStudio";
}

export function getTheme(themeId: ThemeId) {
  return themeRegistry[themeId];
}
