import * as DropdownMenuPrimitive from "@radix-ui/react-dropdown-menu";
import clsx from "clsx";
import { Check, ChevronDown, Search } from "lucide-react";
import {
  useMemo,
  useState,
  forwardRef,
  type ButtonHTMLAttributes,
  type ChangeEvent,
  type CompositionEvent,
  type KeyboardEvent,
  type ReactNode,
  type Ref,
} from "react";

export type ToolbarViewMode = "list" | "columns" | "grid";

export interface ToolbarViewOption<Value extends ToolbarViewMode = ToolbarViewMode> {
  icon: ReactNode;
  label: string;
  value: Value;
}

export interface ToolbarSelectOption<Value extends string = string> {
  label: string;
  value: Value;
}

export function DataToolbar({
  actions,
  ariaLabel,
  className,
  compact = false,
  leading,
  sticky = false,
  stickyBleed = false,
}: {
  actions: ReactNode;
  ariaLabel: string;
  className?: string;
  compact?: boolean;
  leading: ReactNode;
  sticky?: boolean;
  stickyBleed?: boolean;
}) {
  return (
    <section
      aria-label={ariaLabel}
      className={clsx(
        "grid w-full min-w-0 grid-cols-[minmax(0,1fr)_auto] items-center gap-3 overflow-hidden",
        compact && "gap-2",
        sticky &&
          "sticky top-[calc(var(--app-toolbar-top)+var(--app-notification-offset,0px))] z-10 border-b border-theme-card-border bg-theme-toolbar/85 px-[var(--app-page-x)] py-[var(--app-toolbar-y)] shadow-[0_12px_28px_rgb(var(--theme-panel-shadow)/0.18)] backdrop-blur",
        sticky && stickyBleed && "toolbar-bleed -mx-[var(--app-page-x)]",
        className,
      )}
      data-toolbar-root=""
    >
      <div
        className={clsx("toolbar-overflow-viewport flex min-w-0 flex-nowrap items-center gap-3 overflow-x-auto overflow-y-hidden", compact && "gap-2")}
        data-toolbar-leading=""
      >
        {leading}
      </div>
      <div className="flex min-w-max shrink-0 flex-nowrap items-center justify-end gap-2" data-toolbar-actions="">
        {actions}
      </div>
    </section>
  );
}

export function ToolbarCluster({
  ariaLabel,
  children,
  className,
}: {
  ariaLabel: string;
  children: ReactNode;
  className?: string;
}) {
  return (
    <div
      aria-label={ariaLabel}
      className={clsx(
        "toolbar-overflow-viewport inline-flex min-h-10 min-w-0 max-w-full flex-nowrap items-center gap-2 overflow-x-auto overflow-y-hidden rounded-xl border border-theme-control-border bg-theme-control/95 px-3 py-1.5 text-body-sm text-theme-control-fg shadow-[var(--theme-shadow-control-inset)] [&>*]:shrink-0 [&>*]:whitespace-nowrap",
        className,
      )}
      role="group"
    >
      {children}
    </div>
  );
}

export function ToolbarSearch({
  ariaLabel,
  className,
  defaultValue,
  inputRef,
  onChange,
  onCompositionEnd,
  onCompositionStart,
  onKeyDown,
  placeholder,
  trailing,
  value,
}: {
  ariaLabel?: string;
  className?: string;
  defaultValue?: string;
  inputRef?: Ref<HTMLInputElement>;
  onChange: (value: string, event: ChangeEvent<HTMLInputElement>) => void;
  onCompositionEnd?: (event: CompositionEvent<HTMLInputElement>) => void;
  onCompositionStart?: (event: CompositionEvent<HTMLInputElement>) => void;
  onKeyDown?: (event: KeyboardEvent<HTMLInputElement>) => void;
  placeholder: string;
  trailing?: ReactNode;
  value?: string;
}) {
  return (
    <div
      className={clsx(
        "flex h-10 min-w-[16rem] shrink-0 items-center gap-2 whitespace-nowrap rounded-xl border border-theme-control-border bg-theme-control/95 px-3 text-outline shadow-[inset_0_1px_0_rgb(var(--theme-inset-highlight)/0.42)] transition-colors focus-within:border-primary/60 focus-within:text-primary",
        className,
      )}
      data-toolbar-control="search"
    >
      <Search size={17} />
      <input
        aria-label={ariaLabel ?? placeholder}
        className="min-w-0 flex-1 whitespace-nowrap border-0 bg-transparent text-body-sm text-on-surface outline-none placeholder:text-outline"
        defaultValue={defaultValue}
        onChange={(event) => onChange(event.target.value, event)}
        onCompositionEnd={onCompositionEnd}
        onCompositionStart={onCompositionStart}
        onKeyDown={onKeyDown}
        placeholder={placeholder}
        ref={inputRef}
        type="search"
        value={value}
      />
      {trailing}
    </div>
  );
}

export function ToolbarViewToggle<Value extends ToolbarViewMode>({
  ariaLabel,
  onChange,
  options,
  value,
}: {
  ariaLabel: string;
  onChange: (value: Value) => void;
  options: ToolbarViewOption<Value>[];
  value: Value;
}) {
  return (
    <div aria-label={ariaLabel} className="flex h-10 shrink-0 items-center rounded-xl border border-theme-control-border bg-theme-control/95 p-1" role="group">
      {options.map((option) => (
        <button
          aria-label={option.label}
          aria-pressed={value === option.value}
          className={clsx(
            "grid size-8 place-items-center rounded-lg text-on-surface-variant transition-colors hover:text-on-surface",
            value === option.value ? "bg-theme-control-hover text-primary" : "hover:bg-theme-control-hover/70",
          )}
          key={option.value}
          onClick={() => onChange(option.value)}
          title={option.label}
          type="button"
        >
          {option.icon}
        </button>
      ))}
    </div>
  );
}

export function ToolbarActionButton({
  disabled = false,
  icon,
  label,
  onClick,
  primary = false,
  text,
}: {
  disabled?: boolean;
  icon: ReactNode;
  label: string;
  onClick?: () => void;
  primary?: boolean;
  text?: string;
}) {
  return (
    <button
      aria-label={label}
      className={clsx(
        "inline-flex h-10 shrink-0 items-center justify-center gap-2 whitespace-nowrap rounded-xl transition-all active:scale-95 disabled:cursor-not-allowed disabled:opacity-55",
        text ? "min-w-[5.75rem] px-3 text-body-sm font-semibold" : "w-10",
        primary
          ? "theme-primary-gradient text-theme-button-primary-fg hover:-translate-y-0.5"
          : "border border-theme-control-border bg-theme-control/95 text-theme-control-fg shadow-[inset_0_1px_0_rgb(var(--theme-inset-highlight)/0.42)] hover:bg-theme-control-hover hover:text-on-surface",
      )}
      disabled={disabled}
      data-toolbar-control="action"
      onClick={onClick}
      title={label}
      type="button"
    >
      {icon}
      {text && <span className="whitespace-nowrap">{text}</span>}
    </button>
  );
}

export function ToolbarTextButton({
  disabled = false,
  icon,
  label,
  onClick,
}: {
  disabled?: boolean;
  icon: ReactNode;
  label: string;
  onClick?: () => void;
}) {
  return (
    <button
      className="inline-flex h-10 shrink-0 items-center justify-center gap-2 whitespace-nowrap rounded-xl border border-theme-control-border bg-theme-control/95 px-3 text-body-sm text-theme-control-fg shadow-[inset_0_1px_0_rgb(var(--theme-inset-highlight)/0.42)] transition-colors hover:bg-theme-control-hover hover:text-on-surface disabled:cursor-not-allowed disabled:opacity-55"
      disabled={disabled}
      data-toolbar-control="text"
      onClick={onClick}
      type="button"
    >
      {icon}
      <span>{label}</span>
    </button>
  );
}

export function ToolbarMultiSelectDropdown<Value extends string>({
  allLabel,
  ariaLabel,
  clearLabel,
  emptyLabel,
  icon,
  label,
  onClear,
  onToggleValue,
  options,
  selectedValues,
}: {
  allLabel: string;
  ariaLabel: string;
  clearLabel: string;
  emptyLabel: string;
  icon?: ReactNode;
  label: string;
  onClear: () => void;
  onToggleValue: (value: Value) => void;
  options: ToolbarSelectOption<Value>[];
  selectedValues: Value[];
}) {
  const [open, setOpen] = useState(false);
  const selectedSet = useMemo(() => new Set(selectedValues), [selectedValues]);
  const selectedCount = selectedValues.length;

  return (
    <DropdownMenuPrimitive.Root onOpenChange={setOpen} open={open}>
      <DropdownMenuPrimitive.Trigger asChild>
        <ToolbarDropdownButton
          active={selectedCount > 0 || open}
          ariaLabel={ariaLabel}
          expanded={open}
          icon={icon}
          label={selectedCount > 0 ? `${label}(${selectedCount})` : allLabel}
        />
      </DropdownMenuPrimitive.Trigger>
      <DropdownMenuPrimitive.Portal>
        <ToolbarDropdownContent>
          <div className="flex max-h-[min(22rem,var(--radix-dropdown-menu-content-available-height))] flex-col gap-1 overflow-y-auto pr-1">
            <ToolbarDropdownCheckItem checked={selectedCount === 0} label={allLabel} onChange={onClear} />
            {options.length === 0 ? (
              <div className="px-2 py-2 text-body-sm text-outline">{emptyLabel}</div>
            ) : (
              options.map((option) => (
                <ToolbarDropdownCheckItem
                  checked={selectedSet.has(option.value)}
                  key={option.value}
                  label={option.label}
                  onChange={() => onToggleValue(option.value)}
                />
              ))
            )}
          </div>
          {selectedCount > 0 && (
            <>
              <DropdownMenuPrimitive.Separator className="my-2 h-px bg-theme-control-border" />
              <button
                className="h-8 w-full rounded-lg px-2 text-left text-body-sm text-on-surface-variant transition-colors hover:bg-theme-control-hover hover:text-on-surface"
                onClick={onClear}
                type="button"
              >
                {clearLabel}
              </button>
            </>
          )}
        </ToolbarDropdownContent>
      </DropdownMenuPrimitive.Portal>
    </DropdownMenuPrimitive.Root>
  );
}

export function ToolbarSingleSelectDropdown<Value extends string>({
  ariaLabel,
  icon,
  onChange,
  options,
  value,
}: {
  ariaLabel: string;
  icon?: ReactNode;
  onChange: (value: Value) => void;
  options: ToolbarSelectOption<Value>[];
  value: Value;
}) {
  const [open, setOpen] = useState(false);
  const selectedOption = useMemo(() => options.find((option) => option.value === value) ?? null, [options, value]);

  return (
    <DropdownMenuPrimitive.Root onOpenChange={setOpen} open={open}>
      <DropdownMenuPrimitive.Trigger asChild>
        <ToolbarDropdownButton
          active={open}
          ariaLabel={ariaLabel}
          expanded={open}
          icon={icon}
          label={selectedOption?.label ?? ariaLabel}
        />
      </DropdownMenuPrimitive.Trigger>
      <DropdownMenuPrimitive.Portal>
        <ToolbarDropdownContent>
          <div className="flex max-h-[min(22rem,var(--radix-dropdown-menu-content-available-height))] flex-col gap-1 overflow-y-auto pr-1">
            {options.map((option) => {
              const selected = option.value === value;
              return (
                <DropdownMenuPrimitive.Item
                  className={clsx(
                    "grid h-9 cursor-default grid-cols-[minmax(0,1fr)_1rem] items-center gap-3 rounded-lg px-2 text-left text-body-sm outline-none transition-colors",
                    selected
                      ? "bg-theme-control-hover text-primary"
                      : "text-on-surface-variant hover:bg-theme-control-hover hover:text-on-surface",
                  )}
                  key={option.value}
                  onClick={() => {
                    onChange(option.value);
                  }}
                >
                  <span className="min-w-0 truncate">{option.label}</span>
                  {selected && <Check size={15} />}
                </DropdownMenuPrimitive.Item>
              );
            })}
          </div>
        </ToolbarDropdownContent>
      </DropdownMenuPrimitive.Portal>
    </DropdownMenuPrimitive.Root>
  );
}

export function ToolbarSortDirectionButton({
  direction,
  label,
  onClick,
  title,
}: {
  direction: "asc" | "desc";
  label: string;
  onClick: () => void;
  title: string;
}) {
  return (
    <button
      aria-label={label}
      className="inline-flex h-10 w-10 shrink-0 items-center justify-center rounded-xl border border-theme-control-border bg-theme-control/95 text-body-sm font-semibold text-theme-control-fg shadow-[inset_0_1px_0_rgb(var(--theme-inset-highlight)/0.42)] transition-colors hover:bg-theme-control-hover hover:text-on-surface"
      data-toolbar-control="sort-direction"
      onClick={onClick}
      title={title}
      type="button"
    >
      {direction === "desc" ? "↓" : "↑"}
    </button>
  );
}

export function ToolbarSeparator() {
  return <span className="mx-1 h-6 w-px shrink-0 bg-theme-control-border" aria-hidden="true" />;
}

const ToolbarDropdownButton = forwardRef<HTMLButtonElement, ToolbarDropdownButtonProps>(function ToolbarDropdownButton({
  active,
  ariaLabel,
  expanded,
  icon,
  label,
  ...buttonProps
}, ref) {
  return (
    <button
      {...buttonProps}
      aria-label={ariaLabel}
      className={clsx(
        "inline-flex h-10 max-w-[13rem] shrink-0 items-center justify-center gap-2 whitespace-nowrap rounded-xl border px-3 text-body-sm shadow-[inset_0_1px_0_rgb(var(--theme-inset-highlight)/0.42)] transition-colors",
        active
          ? "border-primary/45 bg-theme-control-hover text-primary"
          : "border-theme-control-border bg-theme-control/95 text-theme-control-fg hover:bg-theme-control-hover hover:text-on-surface",
        buttonProps.className,
      )}
      data-toolbar-control="dropdown"
      ref={ref}
      type="button"
    >
      {icon}
      <span className="min-w-0 truncate">{label}</span>
      <ChevronDown className={clsx("shrink-0 transition-transform", expanded && "rotate-180")} size={15} />
    </button>
  );
});

interface ToolbarDropdownButtonProps extends Omit<ButtonHTMLAttributes<HTMLButtonElement>, "children"> {
  active: boolean;
  ariaLabel: string;
  expanded: boolean;
  icon?: ReactNode;
  label: string;
}

function ToolbarDropdownContent({
  children,
}: {
  children: ReactNode;
}) {
  return (
    <DropdownMenuPrimitive.Content
      align="start"
      className="z-30 w-64 rounded-xl border border-theme-card-border bg-theme-card/98 p-2 text-theme-control-fg shadow-[0_18px_44px_rgb(var(--theme-panel-shadow)/0.26)] backdrop-blur"
      collisionPadding={12}
      sideOffset={8}
    >
      {children}
    </DropdownMenuPrimitive.Content>
  );
}

function ToolbarDropdownCheckItem({
  checked,
  label,
  onChange,
}: {
  checked: boolean;
  label: string;
  onChange: () => void;
}) {
  return (
    <DropdownMenuPrimitive.CheckboxItem
      checked={checked}
      className={clsx(
        "grid h-9 cursor-default grid-cols-[1rem_minmax(0,1fr)] items-center gap-3 rounded-lg px-2 text-body-sm outline-none transition-colors",
        checked ? "bg-theme-control-hover text-primary" : "text-on-surface-variant hover:bg-theme-control-hover hover:text-on-surface",
      )}
      onCheckedChange={onChange}
      onSelect={(event) => event.preventDefault()}
    >
      <span className="grid size-3.5 place-items-center rounded border border-theme-control-border bg-theme-control">
        <DropdownMenuPrimitive.ItemIndicator>
          <Check size={11} />
        </DropdownMenuPrimitive.ItemIndicator>
      </span>
      <span className="min-w-0 truncate">{label}</span>
    </DropdownMenuPrimitive.CheckboxItem>
  );
}
