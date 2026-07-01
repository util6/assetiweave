import { cva } from "class-variance-authority";

export const panelRecipe = cva("rounded-xl border text-on-surface", {
  variants: {
    variant: {
      default:
        "border-theme-card-border bg-[linear-gradient(145deg,rgb(var(--theme-card-bg)/0.96),rgb(var(--theme-card-header)/0.92))] shadow-[var(--theme-shadow-card)]",
      muted:
        "border-theme-card-border bg-[linear-gradient(145deg,rgb(var(--theme-card-bg)/0.76),rgb(var(--theme-card-header)/0.68))] shadow-[var(--theme-shadow-card)]",
      inset:
        "border-theme-card-border bg-[linear-gradient(145deg,rgb(var(--theme-card-header)/0.78),rgb(var(--theme-control-bg)/0.66))] shadow-[var(--theme-shadow-control-inset)]",
      toolbar:
        "border-theme-card-border bg-[linear-gradient(135deg,rgb(var(--theme-toolbar-bg)/0.9),rgb(var(--theme-card-header)/0.82))] shadow-[var(--theme-shadow-toolbar)] backdrop-blur",
    },
    padding: {
      none: "p-0",
      sm: "p-3",
      md: "p-4",
      lg: "p-5",
    },
  },
  defaultVariants: {
    variant: "default",
    padding: "md",
  },
});

export const controlRecipe = cva(
  "rounded-lg border border-theme-control-border bg-theme-control text-on-surface shadow-[var(--theme-shadow-control-inset)] transition-colors placeholder:text-outline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary-strong/55 disabled:cursor-not-allowed disabled:opacity-50",
  {
    variants: {
      variant: {
        input: "h-9 px-3 py-2 text-body-sm",
        textarea: "min-h-24 px-3 py-2 text-body-sm",
        select: "h-9 px-3 text-body-sm",
        frame: "px-3 py-3",
      },
    },
    defaultVariants: {
      variant: "input",
    },
  },
);

export const badgeRecipe = cva("inline-flex items-center rounded-md border px-2 py-0.5 text-label-caps uppercase", {
  variants: {
    tone: {
      neutral: "border-theme-control-border bg-theme-control text-on-surface-variant",
      primary: "border-primary/45 bg-primary/10 text-primary",
      create: "border-status-create/35 bg-status-create/15 text-status-create",
      update: "border-status-update/35 bg-status-update/15 text-status-update",
      remove: "border-status-remove/40 bg-status-remove/12 text-status-remove",
      conflict: "border-status-conflict/35 bg-status-conflict/12 text-status-conflict",
    },
  },
  defaultVariants: {
    tone: "neutral",
  },
});

export const dialogRecipe = cva(
  "fixed inset-0 z-50 grid place-items-center bg-[rgb(var(--theme-scrim)/0.56)] px-4 py-6 backdrop-blur-sm",
);

export const iconButtonRecipe = cva(
  "grid place-items-center rounded-lg text-theme-control-fg transition-colors hover:bg-theme-control-hover hover:text-on-surface focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary-strong/55 disabled:cursor-not-allowed disabled:opacity-50",
  {
    variants: {
      size: {
        sm: "size-8",
        md: "size-9",
      },
      framed: {
        true: "border border-theme-control-border bg-theme-control shadow-[var(--theme-shadow-control-inset)]",
        false: "",
      },
      danger: {
        true: "hover:text-status-remove",
        false: "",
      },
    },
    defaultVariants: {
      size: "sm",
      framed: false,
      danger: false,
    },
  },
);

export const surfaceButtonRecipe = cva(
  "inline-flex items-center justify-center gap-2 whitespace-nowrap rounded-xl text-body-sm font-semibold transition-all focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary-strong/55 disabled:pointer-events-none disabled:opacity-50",
  {
    variants: {
      variant: {
        default:
          "theme-primary-gradient text-theme-button-primary-fg hover:-translate-y-px",
        destructive:
          "theme-danger-gradient text-theme-button-primary-fg",
        outline:
          "border border-theme-control-border bg-theme-control text-theme-control-fg shadow-[var(--theme-shadow-control-inset)] hover:bg-theme-control-hover hover:text-on-surface",
        secondary: "bg-theme-control-hover text-on-surface hover:bg-theme-card-header",
        ghost: "text-theme-control-fg hover:bg-theme-control-hover hover:text-on-surface",
        link: "text-primary underline-offset-4 hover:underline",
      },
      size: {
        default: "h-10 px-4 py-2",
        sm: "h-9 rounded-lg px-3",
        lg: "h-11 rounded-xl px-5",
        icon: "size-9",
        "icon-sm": "size-8 rounded-lg",
      },
    },
    defaultVariants: {
      variant: "default",
      size: "default",
    },
  },
);

export const switchRecipe = cva(
  "peer inline-flex h-7 w-12 shrink-0 cursor-pointer items-center rounded-full border border-theme-control-border bg-theme-switch p-0.5 transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary-strong/55 disabled:cursor-not-allowed disabled:opacity-50 data-[state=checked]:border-primary-strong/70 data-[state=checked]:bg-theme-switch-checked",
);

export const switchThumbRecipe = cva(
  "pointer-events-none grid size-5 place-items-center rounded-full bg-theme-switch-thumb transition-transform data-[state=checked]:translate-x-5 data-[state=checked]:bg-theme-switch-checked-thumb",
);
