import {
  Archive,
  Check,
  ChevronLeft,
  ChevronRight,
  History,
  Pencil,
  Quote,
  RefreshCw,
  X,
} from "lucide-react";
import { Badge } from "../foundation/Badge";
import { EmptyState } from "../foundation/EmptyState";
import { Panel } from "../foundation/Panel";
import { MemoryDetailSkeleton } from "./MemorySkeletons";
import { Button } from "../ui/button";
import { useI18n } from "../../i18n/I18nProvider";
import type {
  MemoryItem,
  MemoryItemDetail,
  MemoryItemKind,
  MemoryItemOrigin,
  MemoryItemStatus,
  MemoryRevisionChangeKind,
  MemoryStaleReason,
  MemoryEvidenceSnapshot,
} from "../../types/memory";
import { MemoryFreshnessBadge } from "./MemoryFreshnessBadge";

export function MemoryLibraryWorkspace({
  detail,
  detailError,
  detailLoading,
  items,
  mutationBusy,
  offset,
  onArchive,
  onDirectAccept,
  onEdit,
  onEditAccept,
  onEvidenceOpen,
  onNext,
  onPrevious,
  onReject,
  onRetryDetail,
  onVerify,
  onSelect,
  pageSize,
  selectedItemId,
  totalCount,
}: {
  detail: MemoryItemDetail | null;
  detailError: string | null;
  detailLoading: boolean;
  items: MemoryItem[];
  mutationBusy: boolean;
  offset: number;
  onArchive: () => void;
  onDirectAccept: () => void;
  onEdit: () => void;
  onEditAccept: () => void;
  onEvidenceOpen?: (evidence: MemoryEvidenceSnapshot) => void;
  onNext: () => void;
  onPrevious: () => void;
  onReject: () => void;
  onRetryDetail: () => void;
  onVerify: () => void;
  onSelect: (itemId: string) => void;
  pageSize: number;
  selectedItemId: string | null;
  totalCount: number;
}) {
  const { t } = useI18n();
  const pageCount = Math.max(1, Math.ceil(totalCount / pageSize));
  const currentPage = Math.min(pageCount, Math.floor(offset / pageSize) + 1);
  const canGoPrevious = offset > 0;
  const canGoNext = offset + pageSize < totalCount;

  return (
    <div className="grid min-h-0 flex-1 grid-cols-1 gap-3 overflow-hidden lg:grid-cols-[minmax(18rem,0.85fr)_minmax(24rem,1.35fr)]">
      <Panel className="flex min-h-[18rem] min-w-0 flex-col overflow-hidden" padding="none">
        <div className="flex shrink-0 items-center justify-between gap-3 border-b border-theme-card-border bg-theme-card-header/65 px-4 py-3">
          <div className="min-w-0">
            <h2 className="text-title-sm font-bold text-on-surface">{t("memory.library.listTitle")}</h2>
            <p className="mt-1 text-body-sm text-on-surface-variant">{t("memory.library.total", { count: totalCount })}</p>
          </div>
          <span className="shrink-0 text-label-caps uppercase text-outline">
            {t("memory.library.page", { page: currentPage, total: pageCount })}
          </span>
        </div>

        {items.length > 0 ? (
          <ul aria-label={t("memory.library.listTitle")} className="min-h-0 flex-1 overflow-y-auto p-2" role="list">
            {items.map((item) => (
              <li key={item.id}>
                <button
                  aria-current={selectedItemId === item.id ? "true" : undefined}
                  aria-label={item.title}
                  className={`mb-1.5 flex w-full min-w-0 flex-col gap-2 rounded-lg border px-3 py-3 text-left transition-colors last:mb-0 ${
                    selectedItemId === item.id
                      ? "border-primary/45 bg-theme-control-hover text-on-surface"
                      : "border-transparent text-on-surface-variant hover:border-theme-control-border hover:bg-theme-control/75 hover:text-on-surface"
                  }`}
                  onClick={() => onSelect(item.id)}
                  type="button"
                >
                  <span className="flex w-full min-w-0 items-start justify-between gap-3">
                    <span className="min-w-0 flex-1 truncate text-body-md font-semibold">{item.title}</span>
                    <Badge tone={statusTone(item.status)}>{statusLabel(item.status, t)}</Badge>
                  </span>
                  <span className="line-clamp-2 text-body-sm leading-5 text-on-surface-variant">{item.content_markdown}</span>
                  <span className="flex min-w-0 flex-wrap items-center gap-1.5">
                    <Badge>{kindLabel(item.kind, t)}</Badge>
                    <Badge>{originLabel(item.origin, t)}</Badge>
                    {item.stale_reason ? <Badge tone="conflict">{staleLabel(item.stale_reason, t)}</Badge> : null}
                  </span>
                </button>
              </li>
            ))}
          </ul>
        ) : (
          <EmptyState
            className="m-3 min-h-0 flex-1"
            description={t("memory.library.filteredEmptyDescription")}
            icon={<Quote size={19} />}
            title={t("memory.library.filteredEmptyTitle")}
          />
        )}

        <div className="flex shrink-0 items-center justify-between gap-3 border-t border-theme-card-border bg-theme-card-header/45 px-3 py-2.5">
          <Button disabled={!canGoPrevious} onClick={onPrevious} size="sm" type="button" variant="outline">
            <ChevronLeft size={16} />
            {t("memory.action.previous")}
          </Button>
          <Button disabled={!canGoNext} onClick={onNext} size="sm" type="button" variant="outline">
            {t("memory.action.next")}
            <ChevronRight size={16} />
          </Button>
        </div>
      </Panel>

      <Panel className="min-h-[18rem] min-w-0 overflow-y-auto" padding="none">
        <MemoryDetailPanel
          detail={detail}
          detailError={detailError}
          detailLoading={detailLoading}
          mutationBusy={mutationBusy}
          onArchive={onArchive}
          onDirectAccept={onDirectAccept}
          onEdit={onEdit}
          onEditAccept={onEditAccept}
          onEvidenceOpen={onEvidenceOpen}
          onReject={onReject}
          onRetryDetail={onRetryDetail}
          onVerify={onVerify}
          selectedItemId={selectedItemId}
        />
      </Panel>
    </div>
  );
}

function MemoryDetailPanel({
  detail,
  detailError,
  detailLoading,
  mutationBusy,
  onArchive,
  onDirectAccept,
  onEdit,
  onEditAccept,
  onEvidenceOpen,
  onReject,
  onRetryDetail,
  onVerify,
  selectedItemId,
}: {
  detail: MemoryItemDetail | null;
  detailError: string | null;
  detailLoading: boolean;
  mutationBusy: boolean;
  onArchive: () => void;
  onDirectAccept: () => void;
  onEdit: () => void;
  onEditAccept: () => void;
  onEvidenceOpen?: (evidence: MemoryEvidenceSnapshot) => void;
  onReject: () => void;
  onRetryDetail: () => void;
  onVerify: () => void;
  selectedItemId: string | null;
}) {
  const { locale, t } = useI18n();

  if (!selectedItemId) {
    return <EmptyState className="m-3 min-h-[16rem]" icon={<Quote size={19} />} title={t("memory.detail.noSelection")} />;
  }
  if (detailLoading || detail?.item.id !== selectedItemId) {
    if (detailError) {
      return (
        <EmptyState
          actions={
            <Button onClick={onRetryDetail} size="sm" type="button" variant="outline">
              {t("memory.action.retry")}
            </Button>
          }
          className="m-3 min-h-[16rem]"
          description={detailError}
          icon={<Quote size={19} />}
          role="alert"
          title={t("memory.library.errorTitle")}
        />
      );
    }
    return (
      <div aria-busy="true" className="min-h-0" role="status">
        <span className="sr-only">{t("memory.library.loading")}</span>
        <MemoryDetailSkeleton />
      </div>
    );
  }

  const { evidence, item, revisions } = detail;
  const canEdit = item.status === "active" || item.status === "completed";
  const canArchive = item.status !== "archived" && item.status !== "rejected";
  const candidate = item.status === "candidate";

  return (
    <article aria-label={t("memory.library.detailTitle")} className="flex min-h-full flex-col">
      <header className="border-b border-theme-card-border bg-theme-card-header/65 px-5 py-4">
        <div className="flex min-w-0 flex-wrap items-start justify-between gap-3">
          <div className="min-w-0 flex-1">
            <div className="flex flex-wrap items-center gap-1.5">
              <Badge>{kindLabel(item.kind, t)}</Badge>
              <Badge tone={statusTone(item.status)}>{statusLabel(item.status, t)}</Badge>
              <Badge>{originLabel(item.origin, t)}</Badge>
              <MemoryFreshnessBadge reason={item.stale_reason} />
            </div>
            <h2 className="mt-3 text-h3 text-on-surface">{item.title}</h2>
            <p className="mt-2 text-body-sm text-on-surface-variant">
              {t("memory.detail.updated", { value: formatDate(item.updated_at, locale) })}
            </p>
          </div>
          <div className="flex shrink-0 flex-wrap items-center justify-end gap-2">
            {evidence.length ? <Button disabled={mutationBusy} onClick={onVerify} size="sm" type="button" variant="outline"><RefreshCw size={15} />{t("memory.action.verify")}</Button> : null}
            {candidate ? (
              <>
                <Button disabled={mutationBusy} onClick={onDirectAccept} size="sm" type="button">
                  <Check size={16} />
                  {t("memory.action.directAccept")}
                </Button>
                <Button disabled={mutationBusy} onClick={onEditAccept} size="sm" type="button" variant="outline">
                  <Pencil size={16} />
                  {t("memory.action.editAccept")}
                </Button>
                <Button disabled={mutationBusy} onClick={onReject} size="sm" type="button" variant="destructive">
                  <X size={16} />
                  {t("memory.action.reject")}
                </Button>
              </>
            ) : (
              <>
                {canEdit ? (
                  <Button disabled={mutationBusy} onClick={onEdit} size="sm" type="button" variant="outline">
                    <Pencil size={16} />
                    {t("memory.action.edit")}
                  </Button>
                ) : null}
                {canArchive ? (
                  <Button disabled={mutationBusy} onClick={onArchive} size="sm" type="button" variant="outline">
                    <Archive size={16} />
                    {t("memory.action.archive")}
                  </Button>
                ) : null}
              </>
            )}
          </div>
        </div>
      </header>

      <div className="grid gap-5 px-5 py-4">
        <div className="whitespace-pre-wrap text-body-md leading-7 text-on-surface">{item.content_markdown}</div>

        <section aria-labelledby="memory-scope-title">
          <h3 className="text-title-sm font-bold text-on-surface" id="memory-scope-title">
            {t("memory.detail.scope")}
          </h3>
          <dl className="mt-2 grid gap-2 rounded-lg border border-theme-control-border bg-theme-control/55 p-3 text-body-sm sm:grid-cols-2">
            <ScopeEntry label={t("memory.field.appId")} value={item.scope.app_id} />
            <ScopeEntry label={t("memory.field.sourceId")} value={item.scope.source_id} />
            <ScopeEntry label={t("memory.field.projectPath")} value={displayHomePath(item.scope.project_path)} />
            <ScopeEntry label={t("memory.field.sessionId")} value={item.scope.session_id} />
          </dl>
        </section>

        <section aria-labelledby="memory-metadata-title">
          <h3 className="text-title-sm font-bold text-on-surface" id="memory-metadata-title">
            {t("memory.detail.metadata")}
          </h3>
          <div className="mt-2 flex flex-wrap gap-2 text-body-sm text-on-surface-variant">
            {item.confidence !== null ? <Badge>{t("memory.detail.confidence", { value: item.confidence.toFixed(2) })}</Badge> : null}
            <Badge>{t("memory.detail.sourceRevision", { value: item.source_revision })}</Badge>
            <Badge>{t("memory.detail.verifiedRevision", { value: item.verified_revision })}</Badge>
          </div>
        </section>

        <details className="rounded-lg border border-theme-card-border bg-theme-card-header/35" open>
          <summary className="flex cursor-pointer list-none items-center gap-2 px-3 py-3 text-title-sm font-bold text-on-surface">
            <Quote size={17} />
            {t("memory.detail.evidence")}
            <Badge className="ml-auto">{evidence.length}</Badge>
          </summary>
          <div className="grid gap-2 border-t border-theme-card-border p-3">
            {evidence.length > 0 ? (
              evidence.map((snapshot) => {
                const location = t("memory.evidence.location", {
                  block: snapshot.block_id,
                  kind: snapshot.record_kind,
                  session: snapshot.session_id,
                });
                const content = (
                  <>
                  <p className="text-label-caps uppercase text-outline">
                        {location}
                  </p>
                  {snapshot.source_unavailable ? (
                    <p className="mt-2 text-body-sm text-status-conflict">{t("memory.evidence.sourceUnavailable")}</p>
                  ) : null}
                  <blockquote className="mt-2 whitespace-pre-wrap border-l-2 border-primary/45 pl-3 text-body-sm leading-6 text-on-surface-variant">
                    {snapshot.excerpt}
                  </blockquote>
                  </>
                );
                return onEvidenceOpen ? (
                  <button
                    aria-label={snapshot.excerpt || location}
                    className="rounded-lg border border-theme-control-border bg-theme-control/55 p-3 text-left transition-colors hover:border-primary/45 hover:bg-theme-control-hover focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary-strong/55"
                    key={snapshot.id}
                    onClick={() => onEvidenceOpen(snapshot)}
                    type="button"
                  >
                    {content}
                  </button>
                ) : (
                  <div className="rounded-lg border border-theme-control-border bg-theme-control/55 p-3" key={snapshot.id}>
                    {content}
                  </div>
                );
              })
            ) : (
              <p className="text-body-sm text-on-surface-variant">{t("memory.detail.noEvidence")}</p>
            )}
          </div>
        </details>

        <details className="rounded-lg border border-theme-card-border bg-theme-card-header/35" open>
          <summary className="flex cursor-pointer list-none items-center gap-2 px-3 py-3 text-title-sm font-bold text-on-surface">
            <History size={17} />
            {t("memory.detail.revisions")}
            <Badge className="ml-auto">{revisions.length}</Badge>
          </summary>
          <div className="grid gap-2 border-t border-theme-card-border p-3">
            {revisions.length > 0 ? (
              revisions.map((revision) => (
                <div className="rounded-lg border border-theme-control-border bg-theme-control/55 p-3" key={revision.id}>
                  <div className="flex min-w-0 items-center justify-between gap-3">
                    <h4 className="min-w-0 truncate text-body-sm font-semibold text-on-surface">
                      {t("memory.revision.title", {
                        change: revisionLabel(revision.change_kind, t),
                        number: revision.revision_number,
                      })}
                    </h4>
                    <time className="shrink-0 text-label-caps uppercase text-outline">{formatDate(revision.changed_at, locale)}</time>
                  </div>
                  <p className="mt-2 line-clamp-3 whitespace-pre-wrap text-body-sm leading-5 text-on-surface-variant">
                    {revision.content_markdown}
                  </p>
                </div>
              ))
            ) : (
              <p className="text-body-sm text-on-surface-variant">{t("memory.detail.noRevisions")}</p>
            )}
          </div>
        </details>
      </div>
    </article>
  );
}

function ScopeEntry({ label, value }: { label: string; value: string | null }) {
  const { t } = useI18n();
  return (
    <div className="min-w-0">
      <dt className="text-label-caps uppercase text-outline">{label}</dt>
      <dd className="mt-1 truncate text-on-surface" title={value ?? undefined}>
        {value || t("common.none")}
      </dd>
    </div>
  );
}

type Translator = ReturnType<typeof useI18n>["t"];

export function kindLabel(kind: MemoryItemKind, t: Translator) {
  return t(`memory.kind.${kind}`);
}

export function statusLabel(status: MemoryItemStatus, t: Translator) {
  return t(`memory.status.${status}`);
}

export function originLabel(origin: MemoryItemOrigin, t: Translator) {
  return t(`memory.origin.${origin}`);
}

export function staleLabel(reason: MemoryStaleReason, t: Translator) {
  return t(`memory.stale.${reason}`);
}

function revisionLabel(changeKind: MemoryRevisionChangeKind, t: Translator) {
  return t(`memory.revision.${changeKind}`);
}

function statusTone(status: MemoryItemStatus): "neutral" | "create" | "update" | "remove" | "conflict" {
  if (status === "active" || status === "completed") return "create";
  if (status === "candidate") return "update";
  if (status === "rejected" || status === "archived") return "remove";
  if (status === "superseded") return "conflict";
  return "neutral";
}

function formatDate(value: string, locale: "zh" | "en") {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return new Intl.DateTimeFormat(locale === "zh" ? "zh-CN" : "en", {
    dateStyle: "medium",
    timeStyle: "short",
  }).format(date);
}

function displayHomePath(value: string | null) {
  return value?.replace(/^\/Users\/[^/]+(?=\/|$)/, "~") ?? null;
}
