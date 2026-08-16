import {
  Activity,
  AlertTriangle,
  Brain,
  CircleDot,
  Gauge,
  Plus,
  RefreshCw,
  Search,
  Shapes,
  Sparkles,
  WandSparkles,
} from "lucide-react";
import { useEffect, useMemo, useState, type ReactNode } from "react";
import {
  DataToolbar,
  DebouncedToolbarSearch,
  ToolbarActionButton,
  ToolbarMultiSelectDropdown,
  ToolbarSingleSelectDropdown,
  ToolbarTextButton,
  type ToolbarSelectOption,
} from "../../components/common/DataToolbar";
import { ConfirmDialog } from "../../components/common/ConfirmDialog";
import { EmptyState } from "../../components/foundation/EmptyState";
import { PageHeader } from "../../components/foundation/PageHeader";
import {
  MemoryItemEditorDialog,
  type MemoryEditorMode,
  type MemoryEditorValues,
} from "../../components/memory/MemoryItemEditorDialog";
import {
  kindLabel,
  MemoryLibraryWorkspace,
  originLabel,
  statusLabel,
} from "../../components/memory/MemoryLibraryWorkspace";
import { MemoryDreamWorkspace } from "../../components/memory/MemoryDreamWorkspace";
import { MemoryOverviewWorkspace } from "../../components/memory/MemoryOverviewWorkspace";
import { MemoryRecallWorkspace } from "../../components/memory/MemoryRecallWorkspace";
import { MemoryLibraryContentSkeleton } from "../../components/memory/MemorySkeletons";
import { Button } from "../../components/ui/button";
import { useI18n } from "../../i18n/I18nProvider";
import {
  acceptMemoryCandidate,
  archiveMemoryItem,
  createMemoryItem,
  getMemoryItem,
  listMemoryItems,
  rejectMemoryCandidate,
  updateMemoryItem,
  verifyMemoryItems,
} from "../../services/memory";
import type {
  MemoryItemDetail,
  MemoryEvidenceSnapshot,
  MemoryItemKind,
  MemoryItemOrigin,
  MemoryItemPageResult,
  MemoryItemStatus,
} from "../../types/memory";

const MEMORY_PAGE_SIZE = 50;
const MEMORY_KINDS: MemoryItemKind[] = ["preference", "decision", "method", "context", "follow_up"];
const MEMORY_STATUSES: MemoryItemStatus[] = ["candidate", "active", "completed", "superseded", "archived", "rejected"];
const MEMORY_ORIGINS: MemoryItemOrigin[] = ["manual", "auto_dream", "deep_recall", "full_organize"];

export function MemoryPage({
  activeSubNavId,
  onEvidenceOpen,
}: {
  activeSubNavId: string;
  onEvidenceOpen?: (evidence: MemoryEvidenceSnapshot) => void;
}) {
  const { t } = useI18n();
  if (activeSubNavId === "library") return <MemoryLibraryPage onEvidenceOpen={onEvidenceOpen} />;
  if (activeSubNavId === "dreams") return <MemoryWorkspacePage descriptionKey="memory.dreams.description" titleKey="memory.dreams.title"><MemoryDreamWorkspace onEvidenceOpen={onEvidenceOpen} t={t} /></MemoryWorkspacePage>;
  if (activeSubNavId === "overview") return <MemoryWorkspacePage descriptionKey="memory.overview.description" titleKey="memory.overview.title"><MemoryOverviewWorkspace onEvidenceOpen={onEvidenceOpen} t={t} /></MemoryWorkspacePage>;
  if (activeSubNavId === "recall") return <MemoryWorkspacePage descriptionKey="memory.recall.description" titleKey="memory.recall.title"><MemoryRecallWorkspace onEvidenceOpen={onEvidenceOpen} t={t} /></MemoryWorkspacePage>;
  return <IncompleteMemoryView activeSubNavId={activeSubNavId} />;
}

function MemoryWorkspacePage({
  children,
  descriptionKey,
  titleKey,
}: {
  children: ReactNode;
  descriptionKey: "memory.dreams.description" | "memory.overview.description" | "memory.recall.description";
  titleKey: "memory.dreams.title" | "memory.overview.title" | "memory.recall.title";
}) {
  const { t } = useI18n();
  return (
    <section className="flex min-h-0 flex-1 flex-col gap-[var(--app-section-gap)] overflow-hidden px-[var(--app-page-x)] py-[var(--app-page-y)]">
      <PageHeader description={t(descriptionKey)} eyebrow={t("memory.page.eyebrow")} icon={<Brain size={16} />} title={t(titleKey)} />
      {children}
    </section>
  );
}

function IncompleteMemoryView({ activeSubNavId }: { activeSubNavId: string }) {
  const { t } = useI18n();
  const view =
    activeSubNavId === "dreams"
      ? { description: t("memory.dreams.description"), icon: <Sparkles size={20} />, title: t("memory.dreams.title") }
      : activeSubNavId === "recall"
        ? { description: t("memory.recall.description"), icon: <Search size={20} />, title: t("memory.recall.title") }
        : { description: t("memory.overview.description"), icon: <Gauge size={20} />, title: t("memory.overview.title") };

  return (
    <section className="flex min-h-0 flex-1 flex-col gap-[var(--app-section-gap)] overflow-hidden px-[var(--app-page-x)] py-[var(--app-page-y)]">
      <PageHeader description={view.description} eyebrow={t("memory.page.eyebrow")} icon={<Brain size={16} />} title={view.title} />
      <EmptyState className="min-h-0 flex-1" description={t("memory.incomplete.description")} icon={view.icon} title={view.title} />
    </section>
  );
}

function MemoryLibraryPage({ onEvidenceOpen }: { onEvidenceOpen?: (evidence: MemoryEvidenceSnapshot) => void }) {
  const { t } = useI18n();
  const [page, setPage] = useState<MemoryItemPageResult | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(true);
  const [reloadKey, setReloadKey] = useState(0);
  const [detail, setDetail] = useState<MemoryItemDetail | null>(null);
  const [detailError, setDetailError] = useState<string | null>(null);
  const [detailLoading, setDetailLoading] = useState(false);
  const [detailReloadKey, setDetailReloadKey] = useState(0);
  const [selectedItemId, setSelectedItemId] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const [kinds, setKinds] = useState<MemoryItemKind[]>([]);
  const [statuses, setStatuses] = useState<MemoryItemStatus[]>([]);
  const [origins, setOrigins] = useState<MemoryItemOrigin[]>([]);
  const [freshness, setFreshness] = useState<"all" | "stale">("all");
  const [offset, setOffset] = useState(0);
  const [editor, setEditor] = useState<{ detail: MemoryItemDetail | null; mode: MemoryEditorMode } | null>(null);
  const [confirmAction, setConfirmAction] = useState<"archive" | "reject" | null>(null);
  const [mutationBusy, setMutationBusy] = useState(false);
  const [operationError, setOperationError] = useState<string | null>(null);
  const [editorError, setEditorError] = useState<string | null>(null);

  useEffect(() => {
    let cancelled = false;
    setLoading(true);
    setError(null);
    void listMemoryItems({
      kinds,
      limit: MEMORY_PAGE_SIZE,
      offset,
      origins,
      stale_only: freshness === "stale",
      statuses,
    })
      .then((result) => {
        if (!cancelled) {
          setPage(result);
          setSelectedItemId((current) => {
            if (current && result.items.some((item) => item.id === current)) {
              return current;
            }
            return result.items[0]?.id ?? null;
          });
        }
      })
      .catch((loadError) => {
        if (!cancelled) {
          setError(errorMessage(loadError));
        }
      })
      .finally(() => {
        if (!cancelled) {
          setLoading(false);
        }
      });
    return () => {
      cancelled = true;
    };
  }, [freshness, kinds, offset, origins, reloadKey, statuses]);

  const browserPreview = page?.availability === "browser_preview";
  const visibleItems = useMemo(() => {
    const normalizedQuery = query.trim().toLocaleLowerCase();
    if (!normalizedQuery || !page) return page?.items ?? [];
    return page.items.filter((item) =>
      `${item.title}\n${item.content_markdown}`.toLocaleLowerCase().includes(normalizedQuery),
    );
  }, [page, query]);

  useEffect(() => {
    setSelectedItemId((current) => {
      if (current && visibleItems.some((item) => item.id === current)) {
        return current;
      }
      return visibleItems[0]?.id ?? null;
    });
  }, [visibleItems]);

  useEffect(() => {
    if (!selectedItemId || page?.availability !== "tauri") {
      setDetail(null);
      setDetailError(null);
      setDetailLoading(false);
      return;
    }
    let cancelled = false;
    setDetail(null);
    setDetailLoading(true);
    setDetailError(null);
    void getMemoryItem(selectedItemId)
      .then((result) => {
        if (!cancelled) setDetail(result);
      })
      .catch((loadError) => {
        if (!cancelled) setDetailError(errorMessage(loadError));
      })
      .finally(() => {
        if (!cancelled) setDetailLoading(false);
      });
    return () => {
      cancelled = true;
    };
  }, [detailReloadKey, page?.availability, selectedItemId]);

  function changeFilter<Value>(value: Value, current: Value[], update: (values: Value[]) => void) {
    setOffset(0);
    update(current.includes(value) ? current.filter((entry) => entry !== value) : [...current, value]);
  }

  const kindOptions: ToolbarSelectOption<MemoryItemKind>[] = MEMORY_KINDS.map((value) => ({ label: kindLabel(value, t), value }));
  const statusOptions: ToolbarSelectOption<MemoryItemStatus>[] = MEMORY_STATUSES.map((value) => ({ label: statusLabel(value, t), value }));
  const originOptions: ToolbarSelectOption<MemoryItemOrigin>[] = MEMORY_ORIGINS.map((value) => ({ label: originLabel(value, t), value }));

  async function runMutation(action: () => Promise<MemoryItemDetail>, errorTarget: "dialog" | "page" = "page") {
    setMutationBusy(true);
    setOperationError(null);
    if (errorTarget === "dialog") setEditorError(null);
    try {
      const result = await action();
      setDetail(result);
      setSelectedItemId(result.item.id);
      setEditor(null);
      setConfirmAction(null);
      setReloadKey((current) => current + 1);
    } catch (mutationError) {
      const message = errorMessage(mutationError);
      if (errorTarget === "dialog") setEditorError(message);
      else setOperationError(message);
    } finally {
      setMutationBusy(false);
    }
  }

  function selectedDetail() {
    return detail?.item.id === selectedItemId ? detail : null;
  }

  function handleEditorSubmit(values: MemoryEditorValues) {
    if (!editor) return;
    if (editor.mode === "create") {
      void runMutation(() => createMemoryItem(values), "dialog");
      return;
    }
    const itemId = editor.detail?.item.id;
    if (!itemId) return;
    const params = { ...values, item_id: itemId };
    if (editor.mode === "accept") {
      void runMutation(() => acceptMemoryCandidate(params), "dialog");
      return;
    }
    void runMutation(() => updateMemoryItem(params), "dialog");
  }

  return (
    <section className="flex min-h-0 flex-1 flex-col gap-[var(--app-section-gap)] overflow-hidden px-[var(--app-page-x)] py-[var(--app-page-y)]">
      <PageHeader
        description={t("memory.library.description")}
        eyebrow={t("memory.page.eyebrow")}
        icon={<Brain size={16} />}
        title={t("memory.library.title")}
      />

      <DataToolbar
        actions={
          <>
            <ToolbarTextButton
              disabled={loading}
              icon={<RefreshCw className={loading ? "animate-spin" : undefined} size={16} />}
              label={t("memory.action.refresh")}
              onClick={() => setReloadKey((current) => current + 1)}
            />
            <ToolbarActionButton
              disabled={browserPreview || loading || Boolean(error)}
              icon={<Plus size={17} />}
              label={t("memory.action.create")}
              onClick={() => {
                setEditorError(null);
                setEditor({ detail: null, mode: "create" });
              }}
              primary
              text={t("memory.action.create")}
            />
          </>
        }
        ariaLabel={t("memory.library.title")}
        className="shrink-0"
        compact
        leading={
          <>
            <DebouncedToolbarSearch
              ariaLabel={t("memory.search.label")}
              commitDelayMs={200}
              onChange={setQuery}
              placeholder={t("memory.search.placeholder")}
              submitLabel={t("memory.search.submit")}
              value={query}
            />
            <ToolbarMultiSelectDropdown
              allLabel={t("memory.filter.allKinds")}
              ariaLabel={t("memory.filter.kind")}
              clearLabel={t("toolbar.filter.clear")}
              emptyLabel={t("toolbar.filter.empty")}
              icon={<Shapes size={15} />}
              label={t("memory.field.kind")}
              onClear={() => {
                setOffset(0);
                setKinds([]);
              }}
              onToggleValue={(value) => changeFilter(value, kinds, setKinds)}
              options={kindOptions}
              selectedValues={kinds}
            />
            <ToolbarMultiSelectDropdown
              allLabel={t("memory.filter.allStatuses")}
              ariaLabel={t("memory.filter.status")}
              clearLabel={t("toolbar.filter.clear")}
              emptyLabel={t("toolbar.filter.empty")}
              icon={<CircleDot size={15} />}
              label={t("memory.filter.status")}
              onClear={() => {
                setOffset(0);
                setStatuses([]);
              }}
              onToggleValue={(value) => changeFilter(value, statuses, setStatuses)}
              options={statusOptions}
              selectedValues={statuses}
            />
            <ToolbarMultiSelectDropdown
              allLabel={t("memory.filter.allOrigins")}
              ariaLabel={t("memory.filter.origin")}
              clearLabel={t("toolbar.filter.clear")}
              emptyLabel={t("toolbar.filter.empty")}
              icon={<WandSparkles size={15} />}
              label={t("memory.filter.origin")}
              onClear={() => {
                setOffset(0);
                setOrigins([]);
              }}
              onToggleValue={(value) => changeFilter(value, origins, setOrigins)}
              options={originOptions}
              selectedValues={origins}
            />
            <ToolbarSingleSelectDropdown
              ariaLabel={t("memory.filter.stale")}
              icon={<Activity size={15} />}
              onChange={(value) => {
                setOffset(0);
                setFreshness(value);
              }}
              options={[
                { label: t("memory.filter.allFreshness"), value: "all" },
                { label: t("memory.filter.staleOnly"), value: "stale" },
              ]}
              value={freshness}
            />
          </>
        }
      />

      {loading ? (
        <MemoryLibraryContentSkeleton label={t("memory.library.loading")} />
      ) : error ? (
        <EmptyState
          actions={
            <Button onClick={() => setReloadKey((current) => current + 1)} size="sm" type="button" variant="outline">
              {t("memory.action.retry")}
            </Button>
          }
          className="min-h-0 flex-1"
          description={error}
          icon={<AlertTriangle size={20} />}
          role="alert"
          title={t("memory.library.errorTitle")}
        />
      ) : browserPreview ? (
        <EmptyState
          className="min-h-0 flex-1"
          description={t("memory.library.browserDescription")}
          icon={<Brain size={20} />}
          title={t("memory.library.browserTitle")}
        />
      ) : page && page.items.length > 0 ? (
        <>
          {operationError ? (
            <div className="rounded-lg border border-status-remove/35 bg-status-remove/10 px-3 py-2 text-body-sm text-status-remove" role="alert">
              {operationError}
            </div>
          ) : null}
          <MemoryLibraryWorkspace
            detail={detail}
            detailError={detailError}
            detailLoading={detailLoading}
            items={visibleItems}
            mutationBusy={mutationBusy}
            offset={page.offset}
            onArchive={() => {
              setOperationError(null);
              setConfirmAction("archive");
            }}
            onDirectAccept={() => {
              const current = selectedDetail();
              if (current) void runMutation(() => acceptMemoryCandidate({ item_id: current.item.id }));
            }}
            onEdit={() => {
              const current = selectedDetail();
              if (current) {
                setEditorError(null);
                setEditor({ detail: current, mode: "edit" });
              }
            }}
            onEditAccept={() => {
              const current = selectedDetail();
              if (current) {
                setEditorError(null);
                setEditor({ detail: current, mode: "accept" });
              }
            }}
            onNext={() => setOffset((current) => current + MEMORY_PAGE_SIZE)}
            onEvidenceOpen={onEvidenceOpen}
            onPrevious={() => setOffset((current) => Math.max(0, current - MEMORY_PAGE_SIZE))}
            onReject={() => {
              setOperationError(null);
              setConfirmAction("reject");
            }}
            onRetryDetail={() => setDetailReloadKey((current) => current + 1)}
            onVerify={() => {
              const current = selectedDetail();
              if (!current) return;
              void runMutation(async () => {
                const result = await verifyMemoryItems([current.item.id]);
                return result.items[0] ?? current;
              });
            }}
            onSelect={(itemId) => {
              setOperationError(null);
              setSelectedItemId(itemId);
            }}
            pageSize={page.limit}
            selectedItemId={selectedItemId}
            totalCount={page.total_count}
          />
        </>
      ) : (
        <EmptyState
          className="min-h-0 flex-1"
          description={t("memory.library.emptyDescription")}
          icon={<Brain size={20} />}
          title={t("memory.library.emptyTitle")}
        />
      )}

      {editor ? (
        <MemoryItemEditorDialog
          busy={mutationBusy}
          detail={editor.detail}
          error={editorError}
          key={`${editor.mode}:${editor.detail?.item.id ?? "new"}`}
          mode={editor.mode}
          onClose={() => {
            setEditor(null);
            setEditorError(null);
          }}
          onSubmit={handleEditorSubmit}
        />
      ) : null}

      <ConfirmDialog
        busy={mutationBusy}
        confirmLabel={t("memory.confirm.archive.confirm")}
        message={t("memory.confirm.archive.message")}
        onClose={() => setConfirmAction(null)}
        onConfirm={() => {
          const current = selectedDetail();
          if (current) void runMutation(() => archiveMemoryItem(current.item.id));
        }}
        open={confirmAction === "archive"}
        title={t("memory.confirm.archive.title")}
      >
        {confirmAction === "archive" && operationError ? (
          <p className="text-body-sm text-status-remove" role="alert">
            {operationError}
          </p>
        ) : null}
      </ConfirmDialog>
      <ConfirmDialog
        busy={mutationBusy}
        confirmLabel={t("memory.confirm.reject.confirm")}
        message={t("memory.confirm.reject.message")}
        onClose={() => setConfirmAction(null)}
        onConfirm={() => {
          const current = selectedDetail();
          if (current) void runMutation(() => rejectMemoryCandidate(current.item.id));
        }}
        open={confirmAction === "reject"}
        title={t("memory.confirm.reject.title")}
        tone="danger"
      >
        {confirmAction === "reject" && operationError ? (
          <p className="text-body-sm text-status-remove" role="alert">
            {operationError}
          </p>
        ) : null}
      </ConfirmDialog>
    </section>
  );
}

function errorMessage(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}
