import React, { useState, useMemo } from "react";
import {
  Search,
  Check,
  Edit2,
  X,
  CheckCircle2,
  ChevronLeft,
  ChevronRight,
  Database,
  Sparkles,
  Layers,
  Link2,
  RotateCcw,
  Unlock,
  EyeOff,
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Panel } from "@/components/foundation/Panel";
import { Badge } from "@/components/foundation/Badge";
import { DialogFrame } from "@/components/foundation/DialogFrame";
import { TruncatedText } from "@/components/foundation/TruncatedText";
import { PillTabs } from "@/components/common/PillTabs";
import type { ModelPriceEntry } from "@/types/usage";

const PAGE_SIZE = 15;

type TabViewMode = "local" | "all";

interface ModelPricingTableProps {
  localModels: ModelPriceEntry[];
  allModels: ModelPriceEntry[];
  onSaveModel: (entry: ModelPriceEntry) => Promise<void>;
  onRebindModel?: (
    rawModel: string,
    targetModelId: string | null,
  ) => Promise<void>;
  onSetPriceLock?: (modelId: string, locked: boolean) => Promise<void>;
  onSetModelIgnored?: (
    rawModel: string,
    provider: string | null,
    ignored: boolean,
  ) => Promise<void>;
  savingModel: boolean;
  notice: string | null;
}

function getRawModelName(entry: ModelPriceEntry): string {
  return entry.rawModel || entry.modelPatterns[0] || "";
}

function getReferenceModelId(entry: ModelPriceEntry): string {
  return entry.matchedModelId || entry.rawModel || entry.modelPatterns[0] || "";
}

function getMatchSourceMeta(entry: ModelPriceEntry): {
  label: string;
  tone: "neutral" | "primary" | "create" | "update";
} {
  if (entry.isIgnored || entry.matchSource === "ignored") {
    return { label: "已忽略", tone: "neutral" };
  }
  if (entry.isManualRebind || entry.matchSource === "manual_rebind") {
    return { label: "手动改绑", tone: "primary" };
  }
  if (entry.matchSource === "user_override") {
    return { label: "自定义锁定", tone: "primary" };
  }
  if (entry.matchSource === "auto_matched") {
    return { label: "自动匹配", tone: "create" };
  }
  return { label: "未匹配", tone: "update" };
}

function getLocalPricingStatusMeta(entry: ModelPriceEntry): {
  label: string;
  tone: "create" | "update";
} {
  if (entry.isIgnored) {
    return { label: "已定价（免费）", tone: "create" };
  }
  if (
    entry.priceLocked &&
    entry.inputPerMillion === 0 &&
    entry.outputPerMillion === 0
  ) {
    return { label: "已定价（免费）", tone: "create" };
  }
  if (
    !entry.matchedModelId ||
    entry.matchSource === "unmatched" ||
    (!entry.priceLocked &&
      entry.inputPerMillion === 0 &&
      entry.outputPerMillion === 0)
  ) {
    return { label: "未定价", tone: "update" };
  }
  return { label: "已生效", tone: "create" };
}

export const ModelPricingTable: React.FC<ModelPricingTableProps> = ({
  localModels,
  allModels,
  onSaveModel,
  onRebindModel,
  onSetPriceLock,
  onSetModelIgnored,
  savingModel,
  notice,
}) => {
  const [activeTab, setActiveTab] = useState<TabViewMode>("local");
  const [searchQuery, setSearchQuery] = useState<string>("");
  const [page, setPage] = useState<number>(1);
  const [editingKey, setEditingKey] = useState<string | null>(null);
  const [editDraft, setEditDraft] = useState<{
    inputPerMillion: string;
    outputPerMillion: string;
    cacheReadPerMillion: string;
  } | null>(null);
  const [localError, setLocalError] = useState<string | null>(null);

  // Searchable base-model picker state
  const [pickerTargetEntry, setPickerTargetEntry] =
    useState<ModelPriceEntry | null>(null);
  const [pickerSearch, setPickerSearch] = useState<string>("");

  const isLocalTab = activeTab === "local";
  const activeSourceModels = isLocalTab ? localModels : allModels;

  const handleStartEdit = (entry: ModelPriceEntry) => {
    if (!isLocalTab || entry.isIgnored) return;
    const key = `${entry.provider}::${getRawModelName(entry)}`;
    setEditingKey(key);
    setEditDraft({
      inputPerMillion: String(entry.inputPerMillion),
      outputPerMillion: String(entry.outputPerMillion),
      cacheReadPerMillion: String(entry.cacheReadPerMillion),
    });
    setLocalError(null);
  };

  const handleCancelEdit = () => {
    setEditingKey(null);
    setEditDraft(null);
    setLocalError(null);
  };

  const handleSave = async (original: ModelPriceEntry) => {
    if (!editDraft) return;
    const inputVal = parseFloat(editDraft.inputPerMillion);
    const outputVal = parseFloat(editDraft.outputPerMillion);
    const cacheVal = parseFloat(editDraft.cacheReadPerMillion);

    if (
      isNaN(inputVal) ||
      isNaN(outputVal) ||
      isNaN(cacheVal) ||
      inputVal < 0 ||
      outputVal < 0 ||
      cacheVal < 0
    ) {
      setLocalError("单价值必须为非负有效数字");
      return;
    }

    try {
      const updated: ModelPriceEntry = {
        ...original,
        inputPerMillion: inputVal,
        outputPerMillion: outputVal,
        cacheReadPerMillion: cacheVal,
      };
      await onSaveModel(updated);
      setEditingKey(null);
      setEditDraft(null);
      setLocalError(null);
    } catch (err) {
      setLocalError(err instanceof Error ? err.message : String(err));
    }
  };

  const handleOpenPicker = (entry: ModelPriceEntry) => {
    setPickerTargetEntry(entry);
    setPickerSearch("");
    setLocalError(null);
  };

  const handleSelectPickerModel = async (targetModelId: string | null) => {
    if (!pickerTargetEntry || !onRebindModel) return;
    const rawName = getRawModelName(pickerTargetEntry);
    try {
      await onRebindModel(rawName, targetModelId);
      setPickerTargetEntry(null);
      setPickerSearch("");
      setLocalError(null);
    } catch (err) {
      setLocalError(err instanceof Error ? err.message : String(err));
    }
  };

  const filteredModels = useMemo(() => {
    const query = searchQuery.trim().toLowerCase();
    if (!query) return activeSourceModels;
    return activeSourceModels.filter(
      (m) =>
        m.provider.toLowerCase().includes(query) ||
        (m.rawModel ?? "").toLowerCase().includes(query) ||
        (m.matchedModelId ?? "").toLowerCase().includes(query) ||
        m.modelPatterns.some((p) => p.toLowerCase().includes(query)),
    );
  }, [activeSourceModels, searchQuery]);

  const filteredPickerModels = useMemo(() => {
    const query = pickerSearch.trim().toLowerCase();
    if (!query) return allModels;
    return allModels.filter(
      (m) =>
        m.provider.toLowerCase().includes(query) ||
        (m.rawModel ?? "").toLowerCase().includes(query) ||
        (m.matchedModelId ?? "").toLowerCase().includes(query) ||
        m.modelPatterns.some((p) => p.toLowerCase().includes(query)),
    );
  }, [allModels, pickerSearch]);

  const totalPages = Math.max(1, Math.ceil(filteredModels.length / PAGE_SIZE));
  const currentPage = Math.min(page, totalPages);
  const pagedModels = useMemo(() => {
    const start = (currentPage - 1) * PAGE_SIZE;
    return filteredModels.slice(start, start + PAGE_SIZE);
  }, [filteredModels, currentPage]);

  const pillItems = useMemo(
    () => [
      {
        id: "local" as const,
        label: "本地已用模型",
        count: localModels.length,
        icon: <Sparkles className="h-3.5 w-3.5" />,
      },
      {
        id: "all" as const,
        label: "全网参考库",
        count: allModels.length,
        icon: <Layers className="h-3.5 w-3.5" />,
      },
    ],
    [localModels.length, allModels.length],
  );

  return (
    <Panel variant="default" className="p-5 rounded-2xl flex flex-col gap-4">
      {/* 头部：标题与 PillTabs / 检索 */}
      <div className="flex flex-col sm:flex-row sm:items-center justify-between gap-3">
        <div className="flex items-center gap-2.5">
          <span className="grid size-8 place-items-center rounded-xl border border-theme-control-border bg-theme-control text-primary">
            <Database className="h-4 w-4" />
          </span>
          <div>
            <h3 className="text-body-md font-semibold text-on-surface">
              {isLocalTab ? "本地模型资费设置" : "全网模型价目库"}
            </h3>
            <p className="text-caption text-on-surface-variant">
              {isLocalTab
                ? "聚焦本地会话实际使用的原始模型，支持底座改绑、锁定单价（🔒）及忽略异常文本（元/百万 tokens）。"
                : "全网主流云厂商及开源模型基准单价参考（只读）。"}
            </p>
          </div>
        </div>

        <div className="flex flex-wrap items-center gap-2.5">
          <PillTabs
            items={pillItems}
            activeId={activeTab}
            onSelect={(id) => {
              setActiveTab(id as TabViewMode);
              setPage(1);
              setEditingKey(null);
              setEditDraft(null);
              setPickerTargetEntry(null);
            }}
            size="sm"
          />

          <div className="relative w-full sm:w-60">
            <Search className="absolute left-3 top-1/2 -translate-y-1/2 h-4 w-4 text-outline" />
            <Input
              value={searchQuery}
              onChange={(e) => {
                setSearchQuery(e.target.value);
                setPage(1);
              }}
              placeholder="搜索模型或厂商..."
              className="pl-9 h-9 rounded-xl bg-theme-control border-theme-control-border text-caption"
            />
          </div>
        </div>
      </div>

      {(notice || localError) && (
        <div
          className={`flex items-center gap-2 rounded-xl border p-3 text-caption ${
            localError
              ? "border-destructive/30 bg-destructive/10 text-destructive"
              : "border-primary/30 bg-primary/10 text-primary"
          }`}
        >
          <CheckCircle2 className="h-4 w-4 shrink-0" />
          <span>{localError || notice}</span>
        </div>
      )}

      {/* 表格主体 */}
      <div
        tabIndex={0}
        role="region"
        aria-label={isLocalTab ? "本地模型资费表" : "全网参考模型价目表"}
        className="overflow-x-auto rounded-xl border border-theme-card-border/60 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary-strong/55"
      >
        <table className="w-full text-left text-body-sm border-collapse">
          <thead>
            <tr className="border-b border-theme-card-border/60 bg-theme-card-header/60 text-caption font-medium text-on-surface-variant">
              <th scope="col" className="py-2.5 px-3">
                厂商
              </th>
              {isLocalTab ? (
                <>
                  <th scope="col" className="py-2.5 px-3">
                    原始模型名
                  </th>
                  <th scope="col" className="py-2.5 px-3">
                    匹配模型
                  </th>
                </>
              ) : (
                <th scope="col" className="py-2.5 px-3">
                  模型规则
                </th>
              )}
              <th scope="col" className="py-2.5 px-3 text-right">
                输入单价 (¥/1M)
              </th>
              <th scope="col" className="py-2.5 px-3 text-right">
                输出单价 (¥/1M)
              </th>
              <th scope="col" className="py-2.5 px-3 text-right">
                缓存读单价 (¥/1M)
              </th>
              <th scope="col" className="py-2.5 px-3 text-center">
                状态
              </th>
              {isLocalTab && (
                <th scope="col" className="py-2.5 px-3 text-center">
                  操作
                </th>
              )}
            </tr>
          </thead>
          <tbody className="divide-y divide-theme-card-border/40">
            {pagedModels.length === 0 ? (
              <tr>
                <td
                  colSpan={isLocalTab ? 8 : 6}
                  className="py-10 text-center text-on-surface-variant text-caption"
                >
                  {isLocalTab ? (
                    <div className="flex flex-col items-center justify-center gap-2">
                      <Sparkles className="h-6 w-6 text-primary/40" />
                      <p className="font-medium text-on-surface">
                        暂无本地使用的模型记录
                      </p>
                      <p className="text-on-surface-variant max-w-sm">
                        当您在会话中产生交互用量，或从外部数据源导入历史会话后，模型将自动收录于此。
                      </p>
                      {allModels.length > 0 && (
                        <Button
                          variant="outline"
                          size="sm"
                          onClick={() => setActiveTab("all")}
                          className="mt-2 h-9 rounded-xl text-caption"
                        >
                          查看全网参考库 ({allModels.length} 款)
                        </Button>
                      )}
                    </div>
                  ) : (
                    "未找到匹配的模型规则"
                  )}
                </td>
              </tr>
            ) : (
              pagedModels.map((entry, idx) => {
                const rawName = getRawModelName(entry);
                const key = `${entry.provider}::${rawName}::${idx}`;
                const rowEditKey = `${entry.provider}::${rawName}`;
                const isEditing = isLocalTab && editingKey === rowEditKey;
                const matchMeta = getMatchSourceMeta(entry);
                const statusMeta = isLocalTab
                  ? getLocalPricingStatusMeta(entry)
                  : entry.inputPerMillion === 0 && entry.outputPerMillion === 0
                    ? { label: "未定价", tone: "update" as const }
                    : { label: "已生效", tone: "create" as const };
                const baseModelId = getReferenceModelId(entry);
                const isUnmatchedOrDirty =
                  !entry.isIgnored &&
                  (!entry.matchedModelId || entry.matchSource === "unmatched");

                return (
                  <tr
                    key={key}
                    className="hover:bg-theme-control/40 transition-colors"
                  >
                    <td className="py-2.5 px-3 align-middle">
                      <Badge
                        tone="neutral"
                        className="rounded-full px-2 py-0.5 text-caption font-mono uppercase bg-theme-control/60"
                      >
                        {entry.provider}
                      </Badge>
                    </td>

                    {isLocalTab ? (
                      <>
                        {/* 原始模型名 */}
                        <td className="py-2.5 px-3 align-middle font-mono text-code-sm text-on-surface font-medium max-w-[210px]">
                          <TruncatedText>{rawName}</TruncatedText>
                        </td>

                        {/* 匹配模型 + 搜索选底座 + 恢复自动 */}
                        <td className="py-2.5 px-3 align-middle">
                          <div className="flex flex-wrap items-center gap-1.5">
                            <TruncatedText
                              className="font-mono text-code-sm text-on-surface max-w-[180px]"
                              tooltip={entry.matchedModelId || "未匹配"}
                            >
                              {entry.matchedModelId ||
                                (entry.isIgnored ? "已忽略" : "未匹配")}
                            </TruncatedText>
                            <Badge
                              tone={matchMeta.tone}
                              className="rounded-full px-2 py-0.5 text-[11px] font-medium normal-case"
                            >
                              {matchMeta.label}
                            </Badge>
                            {!entry.isIgnored && onRebindModel && (
                              <Button
                                variant="outline"
                                size="sm"
                                disabled={savingModel}
                                onClick={() => handleOpenPicker(entry)}
                                className="h-7 rounded-xl px-2 text-caption"
                                aria-label={`改绑 ${rawName}`}
                              >
                                <Link2 className="h-3 w-3 mr-1" />
                                改绑
                              </Button>
                            )}
                            {entry.isManualRebind &&
                              !entry.isIgnored &&
                              onRebindModel && (
                                <Button
                                  variant="ghost"
                                  size="sm"
                                  disabled={savingModel}
                                  onClick={() =>
                                    void onRebindModel(rawName, null)
                                  }
                                  className="h-7 rounded-xl px-2 text-caption text-on-surface-variant hover:text-primary"
                                  aria-label={`恢复自动 ${rawName}`}
                                >
                                  <RotateCcw className="h-3 w-3 mr-1" />
                                  恢复自动
                                </Button>
                              )}
                          </div>
                        </td>
                      </>
                    ) : (
                      <td className="py-2.5 px-3 align-middle font-mono text-code-sm text-on-surface font-medium max-w-xs">
                        <TruncatedText>
                          {entry.modelPatterns.join(", ")}
                        </TruncatedText>
                      </td>
                    )}

                    <td className="py-2.5 px-3 align-middle text-right font-mono">
                      {isEditing && editDraft ? (
                        <Input
                          type="number"
                          step="0.01"
                          min="0"
                          value={editDraft.inputPerMillion}
                          onChange={(e) =>
                            setEditDraft((d) =>
                              d
                                ? { ...d, inputPerMillion: e.target.value }
                                : null,
                            )
                          }
                          className="h-8 w-24 ml-auto text-right text-code-sm rounded-xl"
                        />
                      ) : (
                        <span className="text-on-surface">
                          ¥{entry.inputPerMillion.toFixed(2)}
                        </span>
                      )}
                    </td>

                    <td className="py-2.5 px-3 align-middle text-right font-mono">
                      {isEditing && editDraft ? (
                        <Input
                          type="number"
                          step="0.01"
                          min="0"
                          value={editDraft.outputPerMillion}
                          onChange={(e) =>
                            setEditDraft((d) =>
                              d
                                ? { ...d, outputPerMillion: e.target.value }
                                : null,
                            )
                          }
                          className="h-8 w-24 ml-auto text-right text-code-sm rounded-xl"
                        />
                      ) : (
                        <span className="text-on-surface">
                          ¥{entry.outputPerMillion.toFixed(2)}
                        </span>
                      )}
                    </td>

                    <td className="py-2.5 px-3 align-middle text-right font-mono">
                      {isEditing && editDraft ? (
                        <Input
                          type="number"
                          step="0.01"
                          min="0"
                          value={editDraft.cacheReadPerMillion}
                          onChange={(e) =>
                            setEditDraft((d) =>
                              d
                                ? { ...d, cacheReadPerMillion: e.target.value }
                                : null,
                            )
                          }
                          className="h-8 w-24 ml-auto text-right text-code-sm rounded-xl"
                        />
                      ) : (
                        <span className="text-on-surface-variant">
                          ¥{entry.cacheReadPerMillion.toFixed(2)}
                        </span>
                      )}
                    </td>

                    <td className="py-2.5 px-3 align-middle text-center">
                      <div className="flex flex-wrap items-center justify-center gap-1.5">
                        <Badge
                          tone={statusMeta.tone}
                          className="rounded-full px-2 py-0.5 text-caption font-medium normal-case"
                        >
                          {statusMeta.label}
                        </Badge>
                        {isLocalTab && entry.isIgnored && (
                          <Badge
                            tone="neutral"
                            className="rounded-full px-2 py-0.5 text-[11px] font-medium normal-case"
                          >
                            已忽略
                          </Badge>
                        )}
                        {isLocalTab && !entry.isIgnored && (
                          <Badge
                            tone={entry.priceLocked ? "primary" : "neutral"}
                            className="rounded-full px-2 py-0.5 text-[11px] font-medium normal-case"
                          >
                            {entry.priceLocked ? "🔒 已锁定" : "🔓 未锁定"}
                          </Badge>
                        )}
                      </div>
                    </td>

                    {isLocalTab && (
                      <td className="py-2.5 px-3 align-middle text-center">
                        {isEditing ? (
                          <div className="flex items-center justify-center gap-1">
                            <Button
                              variant="default"
                              size="sm"
                              onClick={() => void handleSave(entry)}
                              disabled={savingModel}
                              className="h-8 w-8 p-0 rounded-xl shadow-sm"
                              title="保存"
                            >
                              <Check className="h-3.5 w-3.5" />
                            </Button>
                            <Button
                              variant="outline"
                              size="sm"
                              onClick={handleCancelEdit}
                              disabled={savingModel}
                              className="h-8 w-8 p-0 rounded-xl"
                              title="取消"
                            >
                              <X className="h-3.5 w-3.5" />
                            </Button>
                          </div>
                        ) : (
                          <div className="flex items-center justify-center gap-1">
                            {!entry.isIgnored && (
                              <Button
                                variant="ghost"
                                size="sm"
                                disabled={savingModel}
                                onClick={() => handleStartEdit(entry)}
                                className="h-8 px-2 text-caption rounded-xl text-on-surface-variant hover:text-primary"
                              >
                                <Edit2 className="h-3.5 w-3.5 mr-1" />
                                编辑
                              </Button>
                            )}
                            {entry.priceLocked &&
                              !entry.isIgnored &&
                              onSetPriceLock && (
                                <Button
                                  variant="outline"
                                  size="sm"
                                  disabled={savingModel}
                                  onClick={() =>
                                    void onSetPriceLock(baseModelId, false)
                                  }
                                  className="h-8 px-2 text-caption rounded-xl"
                                  title="解锁并回落全网参考单价"
                                >
                                  <Unlock className="h-3.5 w-3.5 mr-1" />
                                  解锁
                                </Button>
                              )}
                            {isUnmatchedOrDirty && onSetModelIgnored && (
                              <Button
                                variant="ghost"
                                size="sm"
                                disabled={savingModel}
                                onClick={() =>
                                  void onSetModelIgnored(
                                    rawName,
                                    entry.provider || null,
                                    true,
                                  )
                                }
                                className="h-8 px-2 text-caption rounded-xl text-on-surface-variant hover:text-destructive"
                                title="忽略该原始模型（不计费）"
                              >
                                <EyeOff className="h-3.5 w-3.5 mr-1" />
                                忽略
                              </Button>
                            )}
                            {entry.isIgnored && onSetModelIgnored && (
                              <Button
                                variant="outline"
                                size="sm"
                                disabled={savingModel}
                                onClick={() =>
                                  void onSetModelIgnored(
                                    rawName,
                                    entry.provider || null,
                                    false,
                                  )
                                }
                                className="h-8 px-2 text-caption rounded-xl"
                                title="恢复该原始模型参与计价"
                              >
                                <RotateCcw className="h-3.5 w-3.5 mr-1" />
                                恢复
                              </Button>
                            )}
                          </div>
                        )}
                      </td>
                    )}
                  </tr>
                );
              })
            )}
          </tbody>
        </table>
      </div>

      {totalPages > 1 && (
        <div className="flex items-center justify-between pt-2">
          <span className="text-caption text-on-surface-variant">
            显示 {(currentPage - 1) * PAGE_SIZE + 1} -{" "}
            {Math.min(currentPage * PAGE_SIZE, filteredModels.length)} 条，共{" "}
            {filteredModels.length} 款
          </span>
          <div className="flex items-center gap-1.5">
            <Button
              variant="outline"
              size="sm"
              onClick={() => setPage((p) => Math.max(1, p - 1))}
              disabled={currentPage <= 1}
              className="h-8 w-8 p-0 rounded-xl"
              aria-label="上一页"
            >
              <ChevronLeft className="h-4 w-4" />
            </Button>
            <span className="text-caption font-mono px-2 text-on-surface">
              {currentPage} / {totalPages}
            </span>
            <Button
              variant="outline"
              size="sm"
              onClick={() => setPage((p) => Math.min(totalPages, p + 1))}
              disabled={currentPage >= totalPages}
              className="h-8 w-8 p-0 rounded-xl"
              aria-label="下一页"
            >
              <ChevronRight className="h-4 w-4" />
            </Button>
          </div>
        </div>
      )}

      {/* Searchable Base-Model Picker Dialog */}
      {pickerTargetEntry && (
        <DialogFrame
          title="选择匹配底座模型"
          description={`原始模型名：${getRawModelName(pickerTargetEntry)}`}
          icon={<Link2 className="h-4 w-4" />}
          layer="nested"
          size="md"
          busy={savingModel}
          onClose={() => setPickerTargetEntry(null)}
          footer={
            <div className="flex w-full items-center justify-between gap-2">
              <div>
                {pickerTargetEntry.isManualRebind && onRebindModel && (
                  <Button
                    variant="outline"
                    disabled={savingModel}
                    onClick={() => void handleSelectPickerModel(null)}
                  >
                    <RotateCcw className="mr-1.5 h-4 w-4" />
                    恢复自动匹配
                  </Button>
                )}
              </div>
              <Button
                variant="outline"
                disabled={savingModel}
                onClick={() => setPickerTargetEntry(null)}
              >
                取消
              </Button>
            </div>
          }
        >
          <div className="flex flex-col gap-3">
            <div className="relative">
              <Search className="absolute left-3 top-1/2 -translate-y-1/2 h-4 w-4 text-outline" />
              <Input
                value={pickerSearch}
                onChange={(e) => setPickerSearch(e.target.value)}
                placeholder="搜索全网参考底座模型或厂商..."
                className="pl-9 h-10 rounded-xl bg-theme-control border-theme-control-border text-body-sm"
                autoFocus
              />
            </div>

            <div className="max-h-72 overflow-y-auto flex flex-col gap-1.5 pr-1">
              {filteredPickerModels.length === 0 ? (
                <div className="py-8 text-center text-caption text-on-surface-variant">
                  未找到匹配的全网底座模型
                </div>
              ) : (
                filteredPickerModels.map((candidate, idx) => {
                  const candidateId = getReferenceModelId(candidate);
                  const isCurrent =
                    pickerTargetEntry.matchedModelId === candidateId;
                  return (
                    <Button
                      key={`${candidate.provider}::${candidateId}::${idx}`}
                      variant="ghost"
                      disabled={savingModel}
                      onClick={() => void handleSelectPickerModel(candidateId)}
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
                        <TruncatedText className="font-mono text-body-sm font-medium text-on-surface">
                          {candidateId}
                        </TruncatedText>
                      </div>
                      <span className="font-mono text-caption text-on-surface-variant shrink-0 ml-2">
                        ¥{candidate.inputPerMillion.toFixed(2)} / ¥
                        {candidate.outputPerMillion.toFixed(2)}
                      </span>
                    </Button>
                  );
                })
              )}
            </div>
          </div>
        </DialogFrame>
      )}
    </Panel>
  );
};
