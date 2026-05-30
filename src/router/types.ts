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

export interface RailMenuItem {
  id: string;
  label: string;
  icon: NavigationIcon;
  scope: MenuScope;
  enabled: boolean;
  position: "primary" | "secondary";
}

export interface HeaderTabItem {
  id: string;
  label: string;
  assetKind?: string;
  enabled: boolean;
}

export interface SubNavItem {
  id: string;
  label: string;
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
