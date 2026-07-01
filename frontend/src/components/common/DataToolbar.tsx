import clsx from "clsx";
import { Search } from "lucide-react";
import type { ChangeEvent, CompositionEvent, KeyboardEvent, ReactNode, Ref } from "react";

export type ToolbarViewMode = "list" | "columns" | "grid";

export interface ToolbarViewOption<Value extends ToolbarViewMode = ToolbarViewMode> {
  icon: ReactNode;
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

export function ToolbarSeparator() {
  return <span className="mx-1 h-6 w-px shrink-0 bg-theme-control-border" aria-hidden="true" />;
}
