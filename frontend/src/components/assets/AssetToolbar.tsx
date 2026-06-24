import type { ReactNode } from "react";
import {
  DataToolbar,
  ToolbarActionButton,
  ToolbarSearch,
  ToolbarSeparator,
  ToolbarTextButton,
  ToolbarViewToggle,
  type ToolbarViewOption,
  type ToolbarViewMode,
} from "../common/DataToolbar";

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

export interface AssetToolbarFilter {
  icon: ReactNode;
  label: string;
  onClick?: () => void;
}

export function AssetToolbar<Value extends AssetToolbarViewMode = AssetToolbarViewMode>({
  actionGroups = [],
  ariaLabel,
  filters = [],
  onQueryChange,
  onViewModeChange,
  query,
  searchClassName = "w-64 max-[1160px]:w-72",
  searchPlaceholder,
  sticky = false,
  stickyBleed = false,
  viewAriaLabel,
  viewMode,
  viewOptions = [],
}: {
  actionGroups?: AssetToolbarAction[][];
  ariaLabel: string;
  filters?: AssetToolbarFilter[];
  onQueryChange: (query: string) => void;
  onViewModeChange?: (viewMode: Value) => void;
  query: string;
  searchClassName?: string;
  searchPlaceholder: string;
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
          <ToolbarSearch
            className={searchClassName}
            onChange={onQueryChange}
            placeholder={searchPlaceholder}
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
          {filters.map((filter) => (
            <ToolbarTextButton icon={filter.icon} key={filter.label} label={filter.label} onClick={filter.onClick} />
          ))}
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
