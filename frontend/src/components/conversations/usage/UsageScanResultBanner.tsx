import React from "react";
import { AlertCircle, CheckCircle2, Info, X } from "lucide-react";
import type { TaskView } from "../../../types/taskCenter";
import type { UsageScanResultSummary } from "../../../types/usage";
import { normalizeUsageScanResult } from "../../../services/usage";
import { IconButton } from "../../common/IconButton";

export type UsageScanNoticeTone = "success" | "info" | "error";

export interface UsageScanNotice {
  tone: UsageScanNoticeTone;
  text: string;
}

const toneBannerClass: Record<UsageScanNoticeTone, string> = {
  success: "border-status-create/35 bg-status-create/10 text-status-create",
  info: "border-status-update/35 bg-status-update/10 text-status-update",
  error: "border-status-remove/35 bg-status-remove/10 text-status-remove",
};

const toneIcon = {
  success: CheckCircle2,
  info: Info,
  error: AlertCircle,
} satisfies Record<UsageScanNoticeTone, typeof CheckCircle2>;

/**
 * 将扫描终态统计转换为用户可见文案，区分全量重扫 / 增量无新增 / 增量新增。
 */
export function buildUsageScanNotice(
  summary: UsageScanResultSummary,
): UsageScanNotice {
  const failedSuffix =
    summary.failedSources > 0
      ? `，${summary.failedSources} 个来源读取失败`
      : "";

  if (summary.mode === "full") {
    return {
      tone: summary.failedSources > 0 ? "info" : "success",
      text: `已全量刷新 ${summary.totalEventsIngested} 条用量事件（重读 ${summary.totalSources} 个来源${failedSuffix}）`,
    };
  }

  if (summary.totalEventsIngested === 0) {
    return {
      tone: summary.failedSources > 0 ? "info" : "success",
      text: `增量扫描完成：无新增用量事件${failedSuffix}`,
    };
  }

  return {
    tone: summary.failedSources > 0 ? "info" : "success",
    text: `增量扫描完成：新增 ${summary.totalEventsIngested} 条用量事件${failedSuffix}`,
  };
}

/** 从任务终态快照生成提示；失败/取消优先透出任务中心错误摘要。 */
export function buildUsageScanNoticeFromTask(
  task: TaskView,
): UsageScanNotice | null {
  if (task.state === "failed") {
    const failureMessage = task.failures[0]?.message;
    return {
      tone: "error",
      text: task.errorSummary || failureMessage || "用量扫描失败",
    };
  }
  if (task.state === "canceled") {
    return { tone: "info", text: "用量扫描已取消" };
  }
  if (task.state !== "succeeded") return null;

  const summary = normalizeUsageScanResult(task.result);
  if (!summary) {
    return { tone: "success", text: "扫描完成，已刷新用量数据" };
  }
  const notice = buildUsageScanNotice(summary);
  if (task.outcome === "partial_success" && notice.tone === "success") {
    return { ...notice, tone: "info" };
  }
  return notice;
}

export const UsageScanResultBanner: React.FC<{
  notice: UsageScanNotice | null;
  onDismiss?: () => void;
}> = ({ notice, onDismiss }) => {
  if (!notice) return null;
  const Icon = toneIcon[notice.tone];

  return (
    <div
      className={`flex items-center gap-2 rounded-xl border p-3.5 text-body-sm backdrop-blur-md ${toneBannerClass[notice.tone]}`}
      role="status"
    >
      <Icon className="size-4 shrink-0" />
      <span className="min-w-0 flex-1">{notice.text}</span>
      {onDismiss ? (
        <IconButton
          compact
          icon={<X className="size-4" />}
          label="关闭扫描提示"
          onClick={onDismiss}
        />
      ) : null}
    </div>
  );
};
