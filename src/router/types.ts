import type { AssetKind } from "../types";

export type NavigationIcon =
  | "archive"
  | "boxes"
  | "brain"
  | "command"
  | "file-code"
  | "gauge"
  | "grid"
  | "layers"
  | "navigation"
  | "rocket"
  | "settings"
  | "shield"
  | "sparkles";

export type MenuScope = "global" | "asset-catalog" | "profile" | "settings";
export type NavigationLocale = "zh" | "en";
export type LocalizedNavigationLabels = Partial<Record<NavigationLocale, string>>;

export interface RailMenuItem {
  id: string;
  label: string;
  labels?: LocalizedNavigationLabels;
  icon: NavigationIcon;
  scope: MenuScope;
  enabled: boolean;
  position: "primary" | "secondary";
}

export interface HeaderTabItem {
  id: string;
  label: string;
  labels?: LocalizedNavigationLabels;
  assetKind?: AssetKind;
  enabled: boolean;
}

export interface SubNavItem {
  id: string;
  label: string;
  labels?: LocalizedNavigationLabels;
  routeKey: string;
  enabled: boolean;
}

export interface NavigationModel {
  activeRailId: string;
  activeHeaderTabId: string;
  activeSubNavId: string;
  railItems: RailMenuItem[];
  headerTabs: HeaderTabItem[];
  subNavItems: Record<string, SubNavItem[]>;
}
