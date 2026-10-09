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
} from "lucide-react";
import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { Panel } from "@/components/foundation/Panel";
import { Badge } from "@/components/foundation/Badge";
import { PillTabs } from "@/components/common/PillTabs";
import type { ModelPriceEntry } from "@/types/usage";

const PAGE_SIZE = 15;

type TabViewMode = "local" | "all";

interface ModelPricingTableProps {
  localModels: ModelPriceEntry[];
  allModels: ModelPriceEntry[];
  onSaveModel: (entry: ModelPriceEntry) => Promise<void>;
  savingModel: boolean;
  notice: string | null;
}

export const ModelPricingTable: React.FC<ModelPricingTableProps> = ({
  localModels,
  allModels,
  onSaveModel,
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

  const activeSourceModels = activeTab === "local" ? localModels : allModels;

  const handleStartEdit = (entry: ModelPriceEntry) => {
    const key = `${entry.provider}::${entry.modelPatterns.join(",")}`;
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

    if (isNaN(inputVal) || isNaN(outputVal) || isNaN(cacheVal)) {
      setLocalError("单价值必须为有效数字");
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

  const filteredModels = useMemo(() => {
    const query = searchQuery.trim().toLowerCase();
    if (!query) return activeSourceModels;
    return activeSourceModels.filter(
      (m) =>
        m.provider.toLowerCase().includes(query) ||
        m.modelPatterns.some((p) => p.toLowerCase().includes(query)),
    );
  }, [activeSourceModels, searchQuery]);

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
              {activeTab === "local" ? "本地模型资费设置" : "全网模型价目库"}
            </h3>
            <p className="text-caption text-on-surface-variant">
              {activeTab === "local"
                ? "聚焦本地会话实际使用的模型，空间紧凑且支持随时手动修正单价（元/百万 tokens）。"
                : "全网主流云厂商及开源模型基准单价参考。"}
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
      <div className="overflow-x-auto rounded-xl border border-theme-card-border/60">
        <table className="w-full text-left text-body-sm border-collapse">
          <thead>
            <tr className="border-b border-theme-card-border/60 bg-theme-card-header/60 text-caption font-medium text-on-surface-variant">
              <th className="py-2.5 px-3">厂商</th>
              <th className="py-2.5 px-3">模型规则</th>
              <th className="py-2.5 px-3 text-right">输入单价 (¥/1M)</th>
              <th className="py-2.5 px-3 text-right">输出单价 (¥/1M)</th>
              <th className="py-2.5 px-3 text-right">缓存读单价 (¥/1M)</th>
              <th className="py-2.5 px-3 text-center">状态</th>
              <th className="py-2.5 px-3 text-center w-24">操作</th>
            </tr>
          </thead>
          <tbody className="divide-y divide-theme-card-border/40">
            {pagedModels.length === 0 ? (
              <tr>
                <td
                  colSpan={7}
                  className="py-10 text-center text-on-surface-variant text-caption"
                >
                  {activeTab === "local" ? (
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
                          className="mt-2 h-8 rounded-xl text-caption"
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
              pagedModels.map((entry) => {
                const key = `${entry.provider}::${entry.modelPatterns.join(",")}`;
                const isEditing = editingKey === key;
                const isUnpriced =
                  entry.inputPerMillion === 0 && entry.outputPerMillion === 0;

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

                    <td className="py-2.5 px-3 align-middle font-mono text-code-sm text-on-surface font-medium max-w-xs truncate">
                      {entry.modelPatterns.join(", ")}
                    </td>

                    <td className="py-2.5 px-3 align-middle text-right font-mono">
                      {isEditing && editDraft ? (
                        <Input
                          type="number"
                          step="0.01"
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
                      {isUnpriced ? (
                        <Badge
                          tone="update"
                          className="rounded-full px-2 py-0.5 text-caption font-medium"
                        >
                          未定价
                        </Badge>
                      ) : (
                        <Badge
                          tone="create"
                          className="rounded-full px-2 py-0.5 text-caption font-medium"
                        >
                          已生效
                        </Badge>
                      )}
                    </td>

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
                        <Button
                          variant="ghost"
                          size="sm"
                          onClick={() => handleStartEdit(entry)}
                          className="h-8 px-2.5 text-caption rounded-xl text-on-surface-variant hover:text-primary"
                        >
                          <Edit2 className="h-3.5 w-3.5 mr-1" />
                          编辑
                        </Button>
                      )}
                    </td>
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
            >
              <ChevronRight className="h-4 w-4" />
            </Button>
          </div>
        </div>
      )}
    </Panel>
  );
};
