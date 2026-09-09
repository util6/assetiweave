import * as SelectPrimitive from "@radix-ui/react-select";
import { Check, ChevronDown, ChevronUp } from "lucide-react";
import * as React from "react";

import { cn } from "@/lib/utils";

type SelectOpenListener = (activeId: string | null) => void;
const selectOpenListeners = new Set<SelectOpenListener>();
let currentActiveSelectId: string | null = null;

function notifySelectOpen(id: string) {
  currentActiveSelectId = id;
  selectOpenListeners.forEach((listener) => listener(id));
}

function notifySelectClose(id: string) {
  if (currentActiveSelectId === id) {
    currentActiveSelectId = null;
    selectOpenListeners.forEach((listener) => listener(null));
  }
}

export interface SelectProps
  extends React.ComponentPropsWithoutRef<typeof SelectPrimitive.Root> {
  id?: string;
}

const Select: React.FC<SelectProps> = ({
  children,
  id,
  open: controlledOpen,
  onOpenChange,
  ...props
}) => {
  const generatedId = React.useId();
  const selectId = id || generatedId;
  const [uncontrolledOpen, setUncontrolledOpen] = React.useState(false);
  const isControlled = controlledOpen !== undefined;
  const isOpen = isControlled ? controlledOpen : uncontrolledOpen;

  React.useEffect(() => {
    const listener: SelectOpenListener = (activeId) => {
      if (activeId && activeId !== selectId) {
        if (!isControlled) {
          setUncontrolledOpen(false);
        }
        onOpenChange?.(false);
      }
    };
    selectOpenListeners.add(listener);
    return () => {
      selectOpenListeners.delete(listener);
      if (currentActiveSelectId === selectId) {
        currentActiveSelectId = null;
      }
    };
  }, [selectId, isControlled, onOpenChange]);

  const handleOpenChange = (nextOpen: boolean) => {
    if (nextOpen) {
      notifySelectOpen(selectId);
    } else {
      notifySelectClose(selectId);
    }
    if (!isControlled) {
      setUncontrolledOpen(nextOpen);
    }
    onOpenChange?.(nextOpen);
  };

  return (
    <SelectPrimitive.Root
      onOpenChange={handleOpenChange}
      open={isOpen}
      {...props}
    >
      {children}
    </SelectPrimitive.Root>
  );
};

const SelectGroup = SelectPrimitive.Group;
const SelectValue = SelectPrimitive.Value;

const SelectTrigger = React.forwardRef<
  React.ElementRef<typeof SelectPrimitive.Trigger>,
  React.ComponentPropsWithoutRef<typeof SelectPrimitive.Trigger> & {
    size?: "sm" | "default";
  }
>(({ className, children, size = "default", ...props }, ref) => (
  <SelectPrimitive.Trigger
    className={cn(
      "group flex w-full items-center justify-between gap-2 rounded-xl border border-theme-control-border bg-theme-control text-on-surface shadow-[var(--theme-shadow-control-inset)] outline-none transition-[background-color,border-color,box-shadow,color] duration-200 placeholder:text-outline focus-visible:border-primary-strong/60 focus-visible:ring-2 focus-visible:ring-primary-strong/25 data-[state=open]:border-primary-strong/60 data-[state=open]:ring-2 data-[state=open]:ring-primary-strong/20 disabled:cursor-not-allowed disabled:opacity-50 [&>span]:line-clamp-1",
      size === "sm" ? "h-9 px-2.5 text-body-sm" : "h-10 px-3 text-body-sm",
      className,
    )}
    ref={ref}
    type="button"
    {...props}
  >
    {children}
    <SelectPrimitive.Icon asChild>
      <ChevronDown className="size-4 shrink-0 text-outline transition-transform duration-200 ease-out group-data-[state=open]:rotate-180" />
    </SelectPrimitive.Icon>
  </SelectPrimitive.Trigger>
));
SelectTrigger.displayName = SelectPrimitive.Trigger.displayName;

const SelectScrollUpButton = React.forwardRef<
  React.ElementRef<typeof SelectPrimitive.ScrollUpButton>,
  React.ComponentPropsWithoutRef<typeof SelectPrimitive.ScrollUpButton>
>(({ className, ...props }, ref) => (
  <SelectPrimitive.ScrollUpButton
    className={cn(
      "flex cursor-default items-center justify-center py-1 text-outline",
      className,
    )}
    ref={ref}
    {...props}
  >
    <ChevronUp className="size-4" />
  </SelectPrimitive.ScrollUpButton>
));
SelectScrollUpButton.displayName = SelectPrimitive.ScrollUpButton.displayName;

const SelectScrollDownButton = React.forwardRef<
  React.ElementRef<typeof SelectPrimitive.ScrollDownButton>,
  React.ComponentPropsWithoutRef<typeof SelectPrimitive.ScrollDownButton>
>(({ className, ...props }, ref) => (
  <SelectPrimitive.ScrollDownButton
    className={cn(
      "flex cursor-default items-center justify-center py-1 text-outline",
      className,
    )}
    ref={ref}
    {...props}
  >
    <ChevronDown className="size-4" />
  </SelectPrimitive.ScrollDownButton>
));
SelectScrollDownButton.displayName =
  SelectPrimitive.ScrollDownButton.displayName;

const SelectContent = React.forwardRef<
  React.ElementRef<typeof SelectPrimitive.Content>,
  React.ComponentPropsWithoutRef<typeof SelectPrimitive.Content> & {
    portalContainer?: HTMLElement | null;
  }
>(
  (
    { className, children, position = "popper", portalContainer, ...props },
    ref,
  ) => (
    <SelectPrimitive.Portal container={portalContainer}>
      <SelectPrimitive.Content
        className={cn(
          "aurora-menu relative z-[100] max-h-96 min-w-[8rem] overflow-hidden rounded-2xl border border-theme-card-border/75 bg-theme-card/95 text-theme-control-fg shadow-[var(--theme-shadow-panel)] backdrop-blur-xl",
          position === "popper" && "my-1",
          className,
        )}
        position={position}
        ref={ref}
        {...props}
      >
        <SelectScrollUpButton />
        <SelectPrimitive.Viewport
          className={cn(
            "p-1.5",
            position === "popper" &&
              "w-full min-w-[var(--radix-select-trigger-width)]",
          )}
        >
          {children}
        </SelectPrimitive.Viewport>
        <SelectScrollDownButton />
      </SelectPrimitive.Content>
    </SelectPrimitive.Portal>
  ),
);
SelectContent.displayName = SelectPrimitive.Content.displayName;

const SelectLabel = React.forwardRef<
  React.ElementRef<typeof SelectPrimitive.Label>,
  React.ComponentPropsWithoutRef<typeof SelectPrimitive.Label>
>(({ className, ...props }, ref) => (
  <SelectPrimitive.Label
    className={cn(
      "px-2 py-1.5 text-caption font-semibold text-outline",
      className,
    )}
    ref={ref}
    {...props}
  />
));
SelectLabel.displayName = SelectPrimitive.Label.displayName;

const SelectItem = React.forwardRef<
  React.ElementRef<typeof SelectPrimitive.Item>,
  React.ComponentPropsWithoutRef<typeof SelectPrimitive.Item>
>(({ className, children, ...props }, ref) => (
  <SelectPrimitive.Item
    className={cn(
      "relative flex w-full cursor-pointer select-none items-center rounded-xl py-2 pl-2.5 pr-8 text-body-sm outline-none transition-[background-color,color] duration-150",
      "text-on-surface-variant",
      // Hover 与聚焦高亮态（覆盖原生 :hover、键盘 :focus 和 Radix data-highlighted）
      "hover:bg-theme-control-hover hover:text-on-surface",
      "focus:bg-theme-control-hover focus:text-on-surface",
      "data-[highlighted]:bg-theme-control-hover data-[highlighted]:text-on-surface",
      // 禁用态
      "data-[disabled]:pointer-events-none data-[disabled]:opacity-50",
      // 选中项：主色加粗文本，带勾选标记，未悬停时轻柔底色
      "data-[state=checked]:font-medium data-[state=checked]:text-primary data-[state=checked]:bg-theme-control-hover/45",
      // 当选中项处于当前悬停/高亮时，呈现完整 hover 底色
      "data-[state=checked]:hover:bg-theme-control-hover data-[state=checked]:data-[highlighted]:bg-theme-control-hover",
      className,
    )}
    ref={ref}
    {...props}
  >
    <span className="absolute right-2.5 flex size-3.5 items-center justify-center">
      <SelectPrimitive.ItemIndicator>
        <Check className="size-4 text-primary" />
      </SelectPrimitive.ItemIndicator>
    </span>
    <SelectPrimitive.ItemText>{children}</SelectPrimitive.ItemText>
  </SelectPrimitive.Item>
));
SelectItem.displayName = SelectPrimitive.Item.displayName;

const SelectSeparator = React.forwardRef<
  React.ElementRef<typeof SelectPrimitive.Separator>,
  React.ComponentPropsWithoutRef<typeof SelectPrimitive.Separator>
>(({ className, ...props }, ref) => (
  <SelectPrimitive.Separator
    className={cn("-mx-1 my-1 h-px bg-theme-control-border", className)}
    ref={ref}
    {...props}
  />
));
SelectSeparator.displayName = SelectPrimitive.Separator.displayName;

export interface SelectOption<T extends string = string> {
  disabled?: boolean;
  label: React.ReactNode;
  value: T;
}

export interface SimpleSelectProps<T extends string = string> {
  ariaLabel?: string;
  className?: string;
  disabled?: boolean;
  icon?: React.ReactNode;
  id?: string;
  onChange: (value: T) => void;
  options: SelectOption<T>[];
  placeholder?: string;
  portalContainer?: HTMLElement | null;
  size?: "sm" | "default";
  triggerClassName?: string;
  value: T;
}

/**
 * 基于 @radix-ui/react-select 的简易包装组件
 */
export function SimpleSelect<T extends string = string>({
  ariaLabel,
  className,
  disabled,
  icon,
  id,
  onChange,
  options,
  placeholder = "请选择...",
  portalContainer,
  size = "default",
  triggerClassName,
  value,
}: SimpleSelectProps<T>) {
  const selectedOption = options.find((option) => option.value === value);

  return (
    <div className={cn("relative inline-block w-full min-w-0", className)}>
      <Select disabled={disabled} onValueChange={onChange} value={value}>
        <SelectTrigger
          aria-label={ariaLabel}
          className={triggerClassName}
          id={id}
          size={size}
        >
          <span className="flex min-w-0 flex-1 items-center gap-2 truncate text-left">
            {icon ? <span className="shrink-0">{icon}</span> : null}
            <SelectValue placeholder={placeholder}>
              {selectedOption ? selectedOption.label : undefined}
            </SelectValue>
          </span>
        </SelectTrigger>
        <SelectContent
          onCloseAutoFocus={(e) => {
            // 避免关闭后强制归还焦点导致输入框、滚动位置或父级 Dialog 状态抖动
            e.preventDefault();
          }}
          portalContainer={portalContainer}
        >
          {options.map((option) => (
            <SelectItem
              disabled={option.disabled}
              key={option.value}
              value={option.value}
            >
              {option.label}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
    </div>
  );
}

export {
  Select,
  SelectContent,
  SelectGroup,
  SelectItem,
  SelectLabel,
  SelectScrollDownButton,
  SelectScrollUpButton,
  SelectSeparator,
  SelectTrigger,
  SelectValue,
};
