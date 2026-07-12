import type { ReactNode } from "react";
import {
  DataToolbar,
  DebouncedToolbarSearch,
  ToolbarActionButton,
  ToolbarSeparator,
  ToolbarViewToggle,
  type ToolbarViewOption,
  type ToolbarViewMode,
} from "../common/DataToolbar";

export const ASSET_TOOLBAR_SEARCH_COMMIT_DELAY_MS = 700;
export type AssetToolbarViewMode = ToolbarViewMode;
export type AssetViewMode = Extract<AssetToolbarViewMode, "list" | "grid">;

export interface AssetToolbarAction {
  disabled?: boolean;
  icon: ReactNode;
  label: string;
  onClick?: () => void;
  primary?: boolean;
  text?: string;
}

export function AssetToolbar<Value extends AssetToolbarViewMode = AssetToolbarViewMode>({
  actionGroups = [],
  ariaLabel,
  filterControls,
  onQueryChange,
  onViewModeChange,
  query,
  searchCommitDelayMs = ASSET_TOOLBAR_SEARCH_COMMIT_DELAY_MS,
  searchClassName = "w-64 max-[1160px]:w-72",
  searchPlaceholder,
  searchSubmitLabel,
  sticky = false,
  stickyBleed = false,
  viewAriaLabel,
  viewMode,
  viewOptions = [],
}: {
  actionGroups?: AssetToolbarAction[][];
  ariaLabel: string;
  filterControls?: ReactNode;
  onQueryChange: (query: string) => void;
  onViewModeChange?: (viewMode: Value) => void;
  query: string;
  searchCommitDelayMs?: number;
  searchClassName?: string;
  searchPlaceholder: string;
  searchSubmitLabel?: string;
  sticky?: boolean;
  stickyBleed?: boolean;
  viewAriaLabel?: string;
  viewMode?: Value;
  viewOptions?: ToolbarViewOption<Value>[];
}) {
  const showViewToggle = viewMode !== undefined && onViewModeChange && viewOptions.length > 0;

  return (
    <DataToolbar
      actions={
        <>
          {actionGroups.map((group, groupIndex) => (
            <ToolbarActionGroup group={group} key={groupIndex} showSeparator={groupIndex > 0} />
          ))}
        </>
      }
      ariaLabel={ariaLabel}
      leading={
        <>
          <DebouncedToolbarSearch
            className={searchClassName}
            commitDelayMs={searchCommitDelayMs}
            onChange={onQueryChange}
            placeholder={searchPlaceholder}
            submitLabel={searchSubmitLabel}
            value={query}
          />
          {showViewToggle && (
            <ToolbarViewToggle
              ariaLabel={viewAriaLabel ?? ariaLabel}
              onChange={onViewModeChange}
              options={viewOptions}
              value={viewMode}
            />
          )}
          {filterControls}
        </>
      }
      sticky={sticky}
      stickyBleed={stickyBleed}
    />
  );
}

function ToolbarActionGroup({
  group,
  showSeparator,
}: {
  group: AssetToolbarAction[];
  showSeparator: boolean;
}) {
  if (group.length === 0) {
    return null;
  }

  return (
    <>
      {showSeparator && <ToolbarSeparator />}
      {group.map((action) => (
        <ToolbarActionButton
          disabled={action.disabled}
          icon={action.icon}
          key={action.label}
          label={action.label}
          onClick={action.onClick}
          primary={action.primary}
          text={action.text}
        />
      ))}
    </>
  );
}
