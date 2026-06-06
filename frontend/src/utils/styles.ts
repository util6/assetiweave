import clsx from "clsx";
import type { AssetKind } from "../types";

export function kindBadgeClass(kind: AssetKind) {
  return clsx(
    "rounded-md px-2 py-0.5 text-[10px] font-bold",
    kind === "skill" && "bg-primary-strong/15 text-primary",
    kind === "rule" && "bg-status-conflict/15 text-status-conflict",
    kind === "agent" && "bg-status-create/15 text-status-create",
    kind !== "skill" && kind !== "rule" && kind !== "agent" && "bg-surface-highest text-on-surface-variant",
  );
}

export function planActionClass(actionType: string) {
  return clsx(
    "rounded-md px-2 py-0.5 text-center text-[10px] font-bold uppercase",
    actionType === "create" && "bg-status-create/15 text-status-create",
    actionType === "update" && "bg-status-update/15 text-status-update",
    actionType === "skip" && "bg-surface-highest text-on-surface-variant",
    actionType === "conflict" && "bg-status-conflict/15 text-status-conflict",
    actionType === "remove" && "bg-status-remove/15 text-status-remove",
  );
}
