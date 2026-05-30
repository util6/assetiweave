import appShortcutIcons from "../../config/appShortcutIcons.json";
import type { AppKind, AppShortcut, AppShortcutIconSvg } from "../../types";

type AppIconKey = Exclude<AppKind, "custom">;

interface AppIconDefinition extends AppShortcutIconSvg {
  legacyIcon?: string;
}

const APP_ICON_TOKEN_PREFIX = "app:";
const APP_ICONS = appShortcutIcons as Record<string, AppIconDefinition>;

export function AppShortcutIcon({
  appKind,
  className,
  displayIcon,
  iconSvg,
}: {
  appKind: AppKind;
  className?: string;
  displayIcon: string;
  iconSvg?: AppShortcutIconSvg | null;
}) {
  const icon = validIconSvg(iconSvg) ? iconSvg : resolveAppIcon(displayIcon, appKind);
  if (!icon) {
    return <span className={className}>{displayIcon.slice(0, 4)}</span>;
  }

  return (
    <svg aria-hidden="true" className={className} fill="currentColor" viewBox={icon.viewBox ?? "0 0 24 24"}>
      {icon.paths.map((path, index) => (
        <path clipRule={path.clipRule} d={path.d} fillRule={path.fillRule} key={`${path.d}-${index}`} />
      ))}
    </svg>
  );
}

export function AppShortcutIconForShortcut({ className, shortcut }: { className?: string; shortcut: AppShortcut }) {
  return <AppShortcutIcon appKind={shortcut.appKind} className={className} displayIcon={shortcut.displayIcon} iconSvg={shortcut.iconSvg} />;
}

export function appIconToken(appKind: AppKind) {
  return supportsAppIcon(appKind) ? `${APP_ICON_TOKEN_PREFIX}${appKind}` : "";
}

export function shortcutUsesAppIcon(shortcut: AppShortcut) {
  return !shortcut.iconSvg && Boolean(resolveAppIcon(shortcut.displayIcon, shortcut.appKind));
}

export function shortcutCustomIconText(shortcut: AppShortcut) {
  return shortcutUsesAppIcon(shortcut) ? "" : shortcut.displayIcon;
}

export function supportsAppIcon(appKind: AppKind): appKind is AppIconKey {
  return appKind in APP_ICONS;
}

function resolveAppIcon(displayIcon: string, appKind: AppKind) {
  const tokenIconKey = parseAppIconToken(displayIcon);
  if (tokenIconKey && tokenIconKey in APP_ICONS) {
    return APP_ICONS[tokenIconKey];
  }

  if (supportsAppIcon(appKind) && displayIcon === APP_ICONS[appKind]?.legacyIcon) {
    return APP_ICONS[appKind];
  }

  return null;
}

function validIconSvg(iconSvg: AppShortcutIconSvg | null | undefined): iconSvg is AppShortcutIconSvg {
  return Boolean(iconSvg?.paths.length);
}

function parseAppIconToken(displayIcon: string): AppIconKey | null {
  if (!displayIcon.startsWith(APP_ICON_TOKEN_PREFIX)) {
    return null;
  }

  const key = displayIcon.slice(APP_ICON_TOKEN_PREFIX.length);
  return isAppIconKey(key) ? key : null;
}

function isAppIconKey(value: string): value is AppIconKey {
  return value in APP_ICONS;
}
