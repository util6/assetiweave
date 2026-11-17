import React, { useMemo, useState } from "react";
import { AlertCircle, ChevronDown, ChevronUp, Info } from "lucide-react";
import type { UsageSourceDiagnostic } from "../../../types/usage";
import { Button } from "../../ui/button";

const COLLAPSED_COUNT = 2;

function describeSource(
  diagnostic: UsageSourceDiagnostic,
  sourceNames: Record<string, string>,
): string {
  return (
    sourceNames[diagnostic.sourceId] ||
    diagnostic.adapterId ||
    diagnostic.sourceId
  );
}

/**
 * 来源诊断横幅：把「某个来源读取失败」如实摆在看板顶部，
 * 避免用户只看到「一切正常」的空图表。
 */
export const UsageSourceDiagnosticsBanner: React.FC<{
  diagnostics: UsageSourceDiagnostic[];
  sourceNames?: Record<string, string>;
}> = ({ diagnostics, sourceNames = {} }) => {
  const [expanded, setExpanded] = useState(false);

  const ordered = useMemo(() => {
    const errors = diagnostics.filter((item) => item.level === "error");
    const rest = diagnostics.filter((item) => item.level !== "error");
    return [...errors, ...rest];
  }, [diagnostics]);

  if (ordered.length === 0) return null;

  const errorCount = ordered.filter((item) => item.level === "error").length;
  const visible = expanded ? ordered : ordered.slice(0, COLLAPSED_COUNT);
  const hiddenCount = ordered.length - visible.length;
  const toneClass =
    errorCount > 0
      ? "border-status-remove/35 bg-status-remove/10 text-status-remove"
      : "border-status-update/35 bg-status-update/10 text-status-update";

  return (
    <div
      className={`flex flex-col gap-2 rounded-xl border p-3.5 text-body-sm backdrop-blur-md ${toneClass}`}
      role="alert"
    >
      <div className="flex items-center gap-2">
        <AlertCircle className="size-4 shrink-0" />
        <span className="min-w-0 flex-1">
          {errorCount > 0
            ? `${errorCount} 个来源读取失败，其余来源数据仍可用`
            : `${ordered.length} 条来源提示`}
        </span>
      </div>

      <ul className="flex flex-col gap-1.5">
        {visible.map((item, index) => (
          <li
            key={`${item.sourceId}-${item.timestamp}-${index}`}
            className="flex min-w-0 items-start gap-2 rounded-xl bg-theme-control/50 px-2.5 py-1.5 text-caption"
          >
            {item.level === "error" ? (
              <AlertCircle className="mt-0.5 size-3.5 shrink-0" />
            ) : (
              <Info className="mt-0.5 size-3.5 shrink-0" />
            )}
            <span className="min-w-0 flex-1 break-words">
              <span className="font-medium">
                {describeSource(item, sourceNames)}
              </span>
              <span className="mx-1 text-on-surface-variant/70">·</span>
              <span className="text-on-surface-variant">{item.message}</span>
            </span>
          </li>
        ))}
      </ul>

      {ordered.length > COLLAPSED_COUNT ? (
        <Button
          variant="ghost"
          size="sm"
          className="h-8 w-fit gap-1 rounded-xl px-2 text-caption"
          onClick={() => setExpanded((prev) => !prev)}
        >
          {expanded ? (
            <>
              <ChevronUp className="size-3.5" />
              收起来源诊断
            </>
          ) : (
            <>
              <ChevronDown className="size-3.5" />
              展开全部 {hiddenCount} 条
            </>
          )}
        </Button>
      ) : null}
    </div>
  );
};
