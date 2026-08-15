import { appShortcutIcons as APP_ICONS } from "../../config/appShortcutIcons";
import type { AppKind, AppShortcut, AppShortcutIconSvg } from "../../types";

type AppIconKey = string;

const APP_ICON_TOKEN_PREFIX = "app:";

export function AppShortcutIcon({
  appKind,
  className,
  displayIcon,
  iconSvg,
  profileId,
  profileName,
}: {
  appKind: AppKind | string;
  className?: string;
  displayIcon: string;
  iconSvg?: AppShortcutIconSvg | null;
  profileId?: string;
  profileName?: string;
}) {
  const icon = validIconSvg(iconSvg) ? iconSvg : resolveAppIcon(displayIcon, appKind, profileId, profileName);
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
  return (
    <AppShortcutIcon
      appKind={shortcut.appKind}
      className={className}
      displayIcon={shortcut.displayIcon}
      iconSvg={shortcut.iconSvg}
      profileId={shortcut.profileId}
      profileName={shortcut.profileName}
    />
  );
}

export function appIconToken(appKindOrKey: AppKind | string) {
  const normalized = appKindOrKey.toLowerCase();
  return supportsAppIcon(normalized) ? `${APP_ICON_TOKEN_PREFIX}${normalized}` : "";
}

export function shortcutUsesAppIcon(shortcut: AppShortcut) {
  return (
    !shortcut.iconSvg &&
    Boolean(resolveAppIcon(shortcut.displayIcon, shortcut.appKind, shortcut.profileId, shortcut.profileName))
  );
}

export function shortcutCustomIconText(shortcut: AppShortcut) {
  return shortcutUsesAppIcon(shortcut) ? "" : shortcut.displayIcon;
}

export function supportsAppIcon(appKindOrKey: string): boolean {
  return appKindOrKey.toLowerCase() in APP_ICONS;
}

export function resolveAppIconKey(
  target:
    | {
        appKind?: string | null;
        profileId?: string | null;
        profileName?: string | null;
        displayIcon?: string | null;
      }
    | string
    | null
    | undefined,
): string | null {
  if (!target) {
    return null;
  }

  if (typeof target === "string") {
    const token = parseAppIconToken(target);
    if (token) return token;
    const normalized = target.trim().toLowerCase();
    if (normalized in APP_ICONS) return normalized;
    return null;
  }

  const { appKind, profileId, profileName, displayIcon } = target;
  if (displayIcon) {
    const token = parseAppIconToken(displayIcon);
    if (token) return token;
  }

  if (appKind && appKind.toLowerCase() in APP_ICONS) {
    return appKind.toLowerCase();
  }

  if (profileId && profileId.toLowerCase() in APP_ICONS) {
    return profileId.toLowerCase();
  }

  if (profileName && profileName.trim().toLowerCase() in APP_ICONS) {
    return profileName.trim().toLowerCase();
  }

  if (displayIcon) {
    for (const [key, asset] of Object.entries(APP_ICONS)) {
      if (displayIcon === asset.legacyIcon) {
        if (
          (profileId && profileId.toLowerCase().includes(key)) ||
          (profileName && profileName.toLowerCase().includes(key))
        ) {
          return key;
        }
      }
    }
  }

  return null;
}

export function resolveAppIcon(
  displayIcon?: string | null,
  appKind?: string | null,
  profileId?: string | null,
  profileName?: string | null,
) {
  if (displayIcon) {
    const tokenIconKey = parseAppIconToken(displayIcon);
    if (tokenIconKey && tokenIconKey in APP_ICONS) {
      return APP_ICONS[tokenIconKey];
    }
  }

  const appKey = resolveAppIconKey({ appKind, profileId, profileName, displayIcon });
  if (appKey && appKey in APP_ICONS) {
    if (
      !displayIcon ||
      displayIcon === APP_ICONS[appKey]?.legacyIcon ||
      displayIcon === appIconToken(appKey) ||
      displayIcon === appKey
    ) {
      return APP_ICONS[appKey];
    }
  }

  if (appKind && supportsAppIcon(appKind) && displayIcon === APP_ICONS[appKind]?.legacyIcon) {
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
