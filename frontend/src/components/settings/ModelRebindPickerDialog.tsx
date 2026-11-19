import React, { useMemo, useState } from "react";
import {
  AlertCircle,
  Layers,
  Link2,
  RotateCcw,
  Search,
  Sparkles,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Badge } from "@/components/foundation/Badge";
import { DialogFrame } from "@/components/foundation/DialogFrame";
import { PillTabs } from "@/components/common/PillTabs";
import type { ModelPriceEntry } from "@/types/usage";

export type RebindPickerSource = "local" | "all";

export function getRawModelName(entry: ModelPriceEntry): string {
  return entry.rawModel || entry.modelPatterns[0] || "";
}

export function getReferenceModelId(entry: ModelPriceEntry): string {
  return entry.matchedModelId || entry.rawModel || entry.modelPatterns[0] || "";
}

export function matchesModelPricingQuery(
  entry: ModelPriceEntry,
  normalizedQuery: string,
): boolean {
  if (!normalizedQuery) return true;
  return (
    entry.provider.toLowerCase().includes(normalizedQuery) ||
    (entry.rawModel ?? "").toLowerCase().includes(normalizedQuery) ||
    (entry.matchedModelId ?? "").toLowerCase().includes(normalizedQuery) ||
    entry.modelPatterns.some((p) => p.toLowerCase().includes(normalizedQuery))
  );
}

function isSelectableLocalTarget(
  candidate: ModelPriceEntry,
  targetEntry: ModelPriceEntry,
): boolean {
  if (candidate.isIgnored || candidate.matchSource === "ignored") {
    return false;
  }
  if (
    candidate.matchSource === "unmatched" ||
    !candidate.matchedModelId?.trim()
  ) {
    return false;
  }
  const sameProvider =
    candidate.provider.trim().toLowerCase() ===
    targetEntry.provider.trim().toLowerCase();
  const sameRawModel =
    getRawModelName(candidate).trim().toLowerCase() ===
    getRawModelName(targetEntry).trim().toLowerCase();
  return !(sameProvider && sameRawModel);
}

interface ModelRebindPickerDialogProps {
  targetEntry: ModelPriceEntry;
  localModels: ModelPriceEntry[];
  allModels: ModelPriceEntry[];
  saving: boolean;
  error?: string | null;
  onSelect: (targetModelId: string | null) => Promise<void>;
  onClose: () => void;
}

export const ModelRebindPickerDialog: React.FC<
  ModelRebindPickerDialogProps
> = ({
  targetEntry,
  localModels,
  allModels,
  saving,
  error,
  onSelect,
  onClose,
}) => {
  const selectableLocalModels = useMemo(
    () => localModels.filter((m) => isSelectableLocalTarget(m, targetEntry)),
    [localModels, targetEntry],
  );

  const [pickerSource, setPickerSource] = useState<RebindPickerSource>(() =>
    selectableLocalModels.length > 0 ? "local" : "all",
  );
  const [pickerSearch, setPickerSearch] = useState<string>("");

  const normalizedQuery = pickerSearch.trim().toLowerCase();

  const filteredLocalModels = useMemo(() => {
    if (!normalizedQuery) return selectableLocalModels;
    return selectableLocalModels.filter((m) =>
      matchesModelPricingQuery(m, normalizedQuery),
    );
  }, [selectableLocalModels, normalizedQuery]);

  const filteredAllModels = useMemo(() => {
    if (!normalizedQuery) return allModels;
    return allModels.filter((m) =>
      matchesModelPricingQuery(m, normalizedQuery),
    );
  }, [allModels, normalizedQuery]);

  const isLocalSource = pickerSource === "local";
  const activeCandidates = isLocalSource
    ? filteredLocalModels
    : filteredAllModels;

  const pillItems = useMemo(
    () => [
      {
        id: "local" as const,
        label: "本地已用模型",
        count: selectableLocalModels.length,
        icon: <Sparkles className="h-3.5 w-3.5" />,
      },
      {
        id: "all" as const,
        label: "全网参考库",
        count: allModels.length,
        icon: <Layers className="h-3.5 w-3.5" />,
      },
    ],
    [selectableLocalModels.length, allModels.length],
  );

  return (
    <DialogFrame
      title="选择匹配底座模型"
      description={`原始模型名：${getRawModelName(targetEntry)}`}
      icon={<Link2 className="h-4 w-4" />}
      layer="nested"
      size="md"
      busy={saving}
      onClose={onClose}
      footer={
        <div className="flex w-full items-center justify-between gap-2">
          <div>
            {targetEntry.isManualRebind && (
              <Button
                variant="outline"
                disabled={saving}
                onClick={() => void onSelect(null)}
              >
                <RotateCcw className="mr-1.5 h-4 w-4" />
                恢复自动匹配
              </Button>
            )}
          </div>
          <Button variant="outline" disabled={saving} onClick={onClose}>
            取消
          </Button>
        </div>
      }
    >
      <div className="flex flex-col gap-3">
        <PillTabs
          items={pillItems}
          activeId={pickerSource}
          onSelect={(id) => setPickerSource(id as RebindPickerSource)}
          ariaLabel="选择改绑模型来源"
          size="sm"
          fullWidth
        />

        <div className="relative">
          <Search className="absolute left-3 top-1/2 -translate-y-1/2 h-4 w-4 text-outline" />
          <Input
            value={pickerSearch}
            onChange={(e) => setPickerSearch(e.target.value)}
            placeholder={
              isLocalSource
                ? "搜索本地已用模型名、匹配底座或厂商..."
                : "搜索全网参考底座模型或厂商..."
            }
            className="pl-9 h-10 rounded-xl bg-theme-control border-theme-control-border text-body-sm"
            autoFocus
          />
        </div>

        {error && (
          <div
            className="flex items-center gap-2 rounded-xl border border-destructive/30 bg-destructive/10 p-3 text-caption text-destructive"
            role="alert"
          >
            <AlertCircle className="h-4 w-4 shrink-0" />
            <span>{error}</span>
          </div>
        )}

        <div className="max-h-72 overflow-y-auto flex flex-col gap-1.5 pr-1">
          {activeCandidates.length === 0 ? (
            <div className="py-8 flex flex-col items-center justify-center gap-2.5 text-center text-caption text-on-surface-variant">
              <span>
                {isLocalSource
                  ? selectableLocalModels.length === 0
                    ? "暂无可用于改绑的已定价本地模型"
                    : "本地已用模型中未找到匹配项"
                  : "未找到匹配的全网底座模型"}
              </span>
              {isLocalSource &&
                (selectableLocalModels.length === 0 ||
                  filteredAllModels.length > 0) && (
                  <Button
                    variant="outline"
                    onClick={() => setPickerSource("all")}
                    className="rounded-xl"
                  >
                    {normalizedQuery && filteredAllModels.length > 0
                      ? `在全网参考库中查看 (${filteredAllModels.length} 款)`
                      : "切换至全网参考库"}
                  </Button>
                )}
            </div>
          ) : (
            activeCandidates.map((candidate, idx) => {
              const candidateId = getReferenceModelId(candidate);
              const candidateRawName = getRawModelName(candidate);
              const showBaseSubline =
                isLocalSource &&
                Boolean(candidate.matchedModelId) &&
                candidate.matchedModelId !== candidateRawName;
              const isCurrent = Boolean(
                targetEntry.matchedModelId &&
                targetEntry.matchedModelId === candidateId,
              );

              return (
                <Button
                  key={`${pickerSource}::${candidate.provider}::${candidateRawName}::${candidateId}::${idx}`}
                  variant="ghost"
                  disabled={saving}
                  onClick={() => void onSelect(candidateId)}
                  className={`h-auto w-full justify-between rounded-xl border px-3 py-2.5 text-left ${
                    isCurrent
                      ? "border-primary/45 bg-primary/10 text-primary"
                      : "border-theme-card-border/60 bg-theme-control/50 hover:border-primary/40 hover:bg-theme-control-hover"
                  }`}
                >
                  <div className="flex items-center gap-2 min-w-0">
                    <Badge
                      tone="neutral"
                      className="rounded-full px-2 py-0.5 text-[11px] font-mono uppercase shrink-0"
                    >
                      {candidate.provider}
                    </Badge>
                    <div className="flex flex-col min-w-0">
                      <span className="font-mono text-body-sm font-medium text-on-surface truncate">
                        {isLocalSource ? candidateRawName : candidateId}
                      </span>
                      {showBaseSubline && (
                        <span className="font-mono text-[11px] text-on-surface-variant truncate">
                          底座: {candidate.matchedModelId}
                        </span>
                      )}
                    </div>
                    {isCurrent && (
                      <Badge
                        tone="primary"
                        className="rounded-full px-2 py-0.5 text-[11px] shrink-0"
                      >
                        当前
                      </Badge>
                    )}
                  </div>
                  <span className="font-mono text-caption text-on-surface-variant shrink-0 ml-2">
                    {isLocalSource && candidate.priceLocked ? "🔒 " : ""}¥
                    {candidate.inputPerMillion.toFixed(2)} / ¥
                    {candidate.outputPerMillion.toFixed(2)}
                  </span>
                </Button>
              );
            })
          )}
        </div>
      </div>
    </DialogFrame>
  );
};
