import { useEffect, useRef, useState } from "react";
import { CheckCircle2, AlertCircle, AlertTriangle, X } from "lucide-react";
import clsx from "clsx";
import {
  useOptionalTaskCenter,
  type TaskNotificationItem,
} from "../../app/backgroundTasks/TaskCenterProvider";
import { useAppUiStore } from "../../store/ui/appUiStore";

export function TaskNotificationBubble() {
  const taskCenter = useOptionalTaskCenter();
  const taskCenterOpen = useAppUiStore((state) => state.taskCenterOpen);

  if (!taskCenter || !taskCenter.currentNotification || taskCenterOpen) {
    return null;
  }

  return (
    <TaskNotificationBubbleContent
      currentNotification={taskCenter.currentNotification}
      dismissNotification={taskCenter.dismissNotification}
      setSelectedTaskId={taskCenter.setSelectedTaskId}
    />
  );
}

function TaskNotificationBubbleContent({
  currentNotification,
  dismissNotification,
  setSelectedTaskId,
}: {
  currentNotification: TaskNotificationItem;
  dismissNotification: (id: string) => void;
  setSelectedTaskId: (id: string | null) => void;
}) {
  const setTaskCenterOpen = useAppUiStore((state) => state.setTaskCenterOpen);

  return (
    <NotificationItem
      key={currentNotification.id}
      notification={currentNotification}
      onDismiss={() => dismissNotification(currentNotification.id)}
      onSelect={() => {
        setSelectedTaskId(currentNotification.task.id);
        dismissNotification(currentNotification.id);
        setTaskCenterOpen(true);
      }}
    />
  );
}

function NotificationItem({
  notification,
  onDismiss,
  onSelect,
}: {
  notification: TaskNotificationItem;
  onDismiss: () => void;
  onSelect: () => void;
}) {
  const [isPaused, setIsPaused] = useState(false);
  const startTimeRef = useRef<number>(Date.now());
  const remainingMsRef = useRef<number>(notification.durationMs);

  useEffect(() => {
    if (isPaused) {
      // Record how much time has passed
      const elapsed = Date.now() - startTimeRef.current;
      remainingMsRef.current = Math.max(0, remainingMsRef.current - elapsed);
      return;
    }

    startTimeRef.current = Date.now();
    const timer = setTimeout(() => {
      onDismiss();
    }, remainingMsRef.current);

    return () => clearTimeout(timer);
  }, [isPaused, onDismiss]);

  const isFailed = notification.isFailure;
  const isPartial = notification.task.outcome === "partial_success";

  return (
    <div
      role="status"
      aria-live="polite"
      className={clsx(
        "pointer-events-auto fixed bottom-24 left-[calc(var(--app-sidebar-width)+12px)] z-50",
        "flex w-[min(22rem,calc(100vw-6rem))] flex-col gap-1.5 rounded-xl border p-3.5 shadow-2xl backdrop-blur-xl transition-all duration-200",
        "cursor-pointer animate-in fade-in slide-in-from-left-4 select-none",
        isFailed
          ? "border-status-remove/40 bg-surface-container/95 text-on-surface shadow-[0_4px_20px_rgb(var(--theme-panel-shadow)/0.2)]"
          : isPartial
            ? "border-status-conflict/40 bg-surface-container/95 text-on-surface shadow-[0_4px_20px_rgb(var(--theme-panel-shadow)/0.2)]"
            : "border-theme-nav-active-border/70 bg-surface-container/95 text-on-surface shadow-primary/10",
      )}
      onMouseEnter={() => setIsPaused(true)}
      onMouseLeave={() => setIsPaused(false)}
      onFocus={() => setIsPaused(true)}
      onBlur={() => setIsPaused(false)}
      onClick={onSelect}
      tabIndex={0}
      onKeyDown={(e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          onSelect();
        }
      }}
    >
      <div className="flex items-start justify-between gap-2">
        <div className="flex items-center gap-2">
          {isFailed ? (
            <AlertCircle className="size-4 shrink-0 text-status-remove" />
          ) : isPartial ? (
            <AlertTriangle className="size-4 shrink-0 text-status-conflict" />
          ) : (
            <CheckCircle2 className="size-4 shrink-0 text-status-create" />
          )}
          <span className="text-body-sm font-semibold truncate leading-tight">
            {notification.title}
          </span>
        </div>
        <button
          aria-label="关闭通知"
          className="grid size-5 shrink-0 place-items-center rounded-full text-on-surface-variant/70 hover:bg-theme-control-hover hover:text-on-surface transition-colors"
          onClick={(e) => {
            e.stopPropagation();
            onDismiss();
          }}
          type="button"
        >
          <X size={14} />
        </button>
      </div>

      <p className="text-body-xs text-on-surface-variant line-clamp-2 pl-6">
        {notification.message}
      </p>

      <div className="flex items-center justify-between pl-6 pt-1 text-[11px] text-on-surface-variant/60 font-mono">
        <span>点击进入任务中心</span>
        <span>{isPaused ? "已暂停" : ""}</span>
      </div>
    </div>
  );
}
