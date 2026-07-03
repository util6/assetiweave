import clsx from "clsx";
import {
  ArrowDownWideNarrow,
  Check,
  ChevronLeft,
  ChevronRight,
  Clock,
  Copy,
  FolderOpen,
  Languages,
  Lightbulb,
  Pencil,
  Plus,
  RefreshCw,
  RotateCw,
  Sparkles,
  Trash2,
} from "lucide-react";
import { useEffect, useMemo, useRef, useState, type CSSProperties, type ReactNode } from "react";
import { EmptyState } from "../../components/foundation/EmptyState";
import { DialogFrame } from "../../components/foundation/DialogFrame";
import { PageHeader } from "../../components/foundation/PageHeader";
import {
  DataToolbar,
  ToolbarActionButton,
  ToolbarSearch,
  ToolbarSingleSelectDropdown,
  ToolbarSortDirectionButton,
} from "../../components/common/DataToolbar";
import { ManualHelpButton } from "../../manuals/ManualHelpButton";
import {
  checkConversationTranslationAvailability,
  translateConversationCardContent,
  type ConversationCardTranslationRequest,
  type OpencodeTranslationAvailability,
  type OpencodeTranslationResult,
} from "../../services/cardTranslation";
import { useI18n } from "../../i18n/I18nProvider";
import { useAppSettings } from "../../store/settings/AppSettingsProvider";
import { normalizeConversationTranslationTargetLanguage } from "../../store/settings/settingsSchema";

export interface PromptNote {
  content: string;
  copyCount: number;
  createdAt: string;
  id: string;
  lastCopiedAt?: string;
  projectPath: string;
  sessionName: string;
  tags: string[];
  title: string;
  optimizedText?: string;
  translatedText?: string;
  updatedAt: string;
}

type PromptAction = "translate" | "optimize";
type PromptCardFace = "front" | "back";
type PromptSortMode = "updated" | "copy-count" | "created" | "title";
type PromptSwitchDirection = "next" | "previous";
type TranslationAvailabilityStatus = "idle" | "checking" | "available" | "unavailable";
type PromptNoteDraft = Pick<PromptNote, "content" | "projectPath" | "sessionName" | "tags" | "title">;

const STORAGE_KEY = "assetiweave.promptNotes";
const COPIED_RESET_MS = 1400;
const OPTIMIZE_PROMPT_TEMPLATE = [
  "You are an expert prompt editor.",
  "Rewrite the content into a clearer, more actionable prompt.",
  "Keep the user's intent, constraints, domain terms, variables, Markdown, and code fences.",
  "Improve structure, remove ambiguity, and make the requested outcome explicit.",
  "Target working language: {targetLanguage}.",
  "Return only the optimized prompt. Do not add commentary.",
  "",
  "<content>",
  "{content}",
  "</content>",
].join("\n");

export function PromptOverviewPage({
  availabilityChecker,
  onManualOpen,
  translator = translateConversationCardContent,
}: {
  availabilityChecker?: () => Promise<OpencodeTranslationAvailability>;
  onManualOpen: () => void;
  translator?: (request: ConversationCardTranslationRequest) => Promise<OpencodeTranslationResult>;
}) {
  const { t } = useI18n();
  const { settings } = useAppSettings();
  const [notes, setNotes] = useState<PromptNote[]>(() => readPromptNotes());
  const [creatingNew, setCreatingNew] = useState(false);
  const [query, setQuery] = useState("");
  const [selectedNoteId, setSelectedNoteId] = useState<string | null>(null);
  const [sortMode, setSortMode] = useState<PromptSortMode>("updated");
  const [sortDirection, setSortDirection] = useState<"asc" | "desc">("desc");
  const [copiedNoteId, setCopiedNoteId] = useState<string | null>(null);
  const [busyActions, setBusyActions] = useState<Record<string, PromptAction | undefined>>({});
  const [availability, setAvailability] = useState<TranslationAvailabilityStatus>("idle");
  const [actionError, setActionError] = useState<string | null>(null);
  const copiedResetTimerRef = useRef<number | null>(null);

  useEffect(() => {
    writePromptNotes(notes);
  }, [notes]);

  useEffect(
    () => () => {
      if (copiedResetTimerRef.current !== null) {
        window.clearTimeout(copiedResetTimerRef.current);
      }
    },
    [],
  );

  useEffect(() => {
    let cancelled = false;
    setAvailability("checking");
    const check = availabilityChecker ?? (() =>
      checkConversationTranslationAvailability({
        cli: settings.conversationTranslation.cli,
        model: settings.conversationTranslation.model,
        provider: settings.conversationTranslation.provider,
      }));

    check()
      .then((result) => {
        if (cancelled) return;
        setAvailability(result.available ? "available" : "unavailable");
      })
      .catch(() => {
        if (cancelled) return;
        setAvailability("unavailable");
      });

    return () => {
      cancelled = true;
    };
  }, [
    availabilityChecker,
    settings.conversationTranslation.cli,
    settings.conversationTranslation.model,
    settings.conversationTranslation.provider,
  ]);

  const filteredNotes = useMemo(() => {
    const normalizedQuery = query.trim().toLowerCase();
    const filtered = notes.filter((note) => {
      if (!normalizedQuery) {
        return true;
      }

      return [note.content, note.optimizedText ?? "", note.projectPath, note.sessionName, note.translatedText ?? "", note.tags.join(" ")]
        .join(" ")
        .toLowerCase()
        .includes(normalizedQuery);
    });
    return sortPromptNotes(filtered, sortMode, sortDirection);
  }, [notes, query, sortDirection, sortMode]);
  const translationTarget = normalizeConversationTranslationTargetLanguage(
    settings.conversationTranslation.targetLanguage,
  );
  const actionsDisabled = availability !== "available";
  const activeNote = creatingNew ? null : filteredNotes.find((note) => note.id === selectedNoteId) ?? filteredNotes[0] ?? null;
  const activeNoteIndex = activeNote ? filteredNotes.findIndex((note) => note.id === activeNote.id) : -1;

  function handleSaveNote(values: PromptNoteDraft, targetFace: PromptCardFace = "front") {
    const normalizedContent = values.content.trim();
    if (!normalizedContent) {
      return;
    }

    const now = new Date().toISOString();
    if (activeNote && !creatingNew) {
      setNotes((current) =>
        current.map((note) =>
          note.id === activeNote.id
            ? {
                ...note,
                content: targetFace === "front" ? normalizedContent : note.content,
                optimizedText: targetFace === "back" ? normalizedContent : note.optimizedText,
                projectPath: values.projectPath.trim(),
                sessionName: values.sessionName.trim(),
                tags: values.tags,
                title: values.title.trim() || t("prompt.note.untitled"),
                updatedAt: now,
              }
            : note,
        ),
      );
      return;
    }

    const note: PromptNote = {
      content: normalizedContent,
      copyCount: 0,
      createdAt: now,
      id: createPromptNoteId(),
      projectPath: values.projectPath.trim(),
      sessionName: values.sessionName.trim(),
      tags: values.tags,
      title: values.title.trim() || t("prompt.note.untitled"),
      updatedAt: now,
    };
    setNotes((current) => [note, ...current]);
    setSelectedNoteId(note.id);
    setCreatingNew(false);
  }

  function handleDeleteNote(noteId: string) {
    setNotes((current) => current.filter((note) => note.id !== noteId));
    setSelectedNoteId((current) => (current === noteId ? null : current));
  }

  async function handleCopyNote(note: PromptNote, text: string) {
    try {
      await navigator.clipboard.writeText(text);
      setNotes((current) =>
        current.map((candidate) =>
          candidate.id === note.id
            ? {
                ...candidate,
                copyCount: candidate.copyCount + 1,
                lastCopiedAt: new Date().toISOString(),
              }
            : candidate,
        ),
      );
      if (copiedResetTimerRef.current !== null) {
        window.clearTimeout(copiedResetTimerRef.current);
      }
      setCopiedNoteId(note.id);
      copiedResetTimerRef.current = window.setTimeout(() => {
        setCopiedNoteId((current) => (current === note.id ? null : current));
        copiedResetTimerRef.current = null;
      }, COPIED_RESET_MS);
    } catch (error) {
      setActionError(t("prompt.action.copyFailed", { message: errorMessage(error) }));
    }
  }

  async function handleTranslateNote(note: PromptNote) {
    await runPromptAction(note, "translate", {
      promptTemplate: settings.conversationTranslation.promptTemplate,
      targetLanguage: settings.conversationTranslation.targetLanguage,
    });
  }

  async function handleOptimizeNote(note: PromptNote, sourceText: string) {
    return runPromptAction(note, "optimize", {
      promptTemplate: OPTIMIZE_PROMPT_TEMPLATE,
      targetLanguage: settings.conversationTranslation.targetLanguage,
      text: sourceText,
    });
  }

  function handleSelectAdjacentNote(offset: number) {
    if (filteredNotes.length === 0) {
      return;
    }

    setCreatingNew(false);
    const currentIndex = activeNoteIndex >= 0 ? activeNoteIndex : 0;
    const nextIndex = (currentIndex + offset + filteredNotes.length) % filteredNotes.length;
    setSelectedNoteId(filteredNotes[nextIndex].id);
  }

  function handleSelectNote(noteId: string) {
    setCreatingNew(false);
    setSelectedNoteId(noteId);
  }

  function handleCreateNewNote() {
    setCreatingNew(true);
    setSelectedNoteId(null);
  }

  async function runPromptAction(
    note: PromptNote,
    action: PromptAction,
    request: Pick<ConversationCardTranslationRequest, "promptTemplate" | "targetLanguage"> & { text?: string },
  ) {
    if (actionsDisabled) {
      return false;
    }

    setActionError(null);
    setBusyActions((current) => ({ ...current, [note.id]: action }));
    try {
      const result = await translator({
        cli: settings.conversationTranslation.cli,
        model: settings.conversationTranslation.model,
        provider: settings.conversationTranslation.provider,
        promptTemplate: request.promptTemplate,
        targetLanguage: request.targetLanguage,
        text: request.text ?? note.content,
      });
      setNotes((current) =>
        current.map((candidate) => {
          if (candidate.id !== note.id) {
            return candidate;
          }

          return {
            ...candidate,
            optimizedText: action === "optimize" ? result.translated_text : candidate.optimizedText,
            translatedText: action === "translate" ? result.translated_text : candidate.translatedText,
            updatedAt: new Date().toISOString(),
          };
        }),
      );
      return true;
    } catch (error) {
      setActionError(
        action === "translate"
          ? t("prompt.action.translateFailed", { message: errorMessage(error) })
          : t("prompt.action.optimizeFailed", { message: errorMessage(error) }),
      );
      return false;
    } finally {
      setBusyActions((current) => {
        const next = { ...current };
        delete next[note.id];
        return next;
      });
    }
  }

  return (
    <section className="flex flex-1 flex-col gap-[var(--app-section-gap)] px-[var(--app-page-x)] py-[var(--app-page-y)]">
      <PageHeader
        eyebrow={t("prompt.page.eyebrow")}
        icon={<Lightbulb size={16} />}
        title={t("prompt.page.title")}
        titleAction={<ManualHelpButton onOpen={onManualOpen} />}
      />

      {actionError ? (
        <div className="rounded-lg border border-status-remove/35 bg-status-remove/10 px-3 py-2 text-body-sm text-status-remove" role="alert">
          {actionError}
        </div>
      ) : null}

      <PromptOverviewToolbar
        onCreateNew={handleCreateNewNote}
        onQueryChange={setQuery}
        onSortDirectionChange={setSortDirection}
        onSortModeChange={setSortMode}
        query={query}
        sortDirection={sortDirection}
        sortMode={sortMode}
      />

      <div className="-mt-4 mx-auto flex w-full max-w-7xl flex-1 flex-col gap-2 overflow-visible">
        <PromptStageCard
          actionsDisabled={actionsDisabled}
          activeIndex={activeNoteIndex}
          activeNote={activeNote}
          busyAction={activeNote ? busyActions[activeNote.id] : undefined}
          copied={activeNote ? copiedNoteId === activeNote.id : false}
          filteredCount={filteredNotes.length}
          notes={filteredNotes}
          onCopyActive={(text) => {
            if (activeNote) {
              void handleCopyNote(activeNote, text);
            }
          }}
          onDeleteActive={() => {
            if (activeNote) {
              handleDeleteNote(activeNote.id);
            }
          }}
          onOptimizeActive={(sourceText) => {
            if (activeNote) {
              return handleOptimizeNote(activeNote, sourceText);
            }
            return Promise.resolve(false);
          }}
          onNextNote={() => handleSelectAdjacentNote(1)}
          onPreviousNote={() => handleSelectAdjacentNote(-1)}
          onSaveActive={handleSaveNote}
          onSelectNote={handleSelectNote}
          onTranslateActive={() => {
            if (activeNote) {
              void handleTranslateNote(activeNote);
            }
          }}
          translationTarget={translationTarget}
        />

        {filteredNotes.length === 0 ? (
          <EmptyState
            description={notes.length === 0 ? t("prompt.empty.description") : t("prompt.empty.filteredDescription")}
            icon={<Lightbulb size={20} />}
            title={notes.length === 0 ? t("prompt.empty.title") : t("prompt.empty.filteredTitle")}
          />
        ) : null}
      </div>
    </section>
  );
}

function PromptOverviewToolbar({
  onCreateNew,
  onQueryChange,
  onSortDirectionChange,
  onSortModeChange,
  query,
  sortDirection,
  sortMode,
}: {
  onCreateNew: () => void;
  onQueryChange: (value: string) => void;
  onSortDirectionChange: (direction: "asc" | "desc") => void;
  onSortModeChange: (mode: PromptSortMode) => void;
  query: string;
  sortDirection: "asc" | "desc";
  sortMode: PromptSortMode;
}) {
  const { t } = useI18n();

  return (
    <DataToolbar
      actions={
        <>
          <ToolbarActionButton
            icon={<Plus size={17} />}
            label={t("prompt.action.new")}
            onClick={onCreateNew}
            primary
            text={t("prompt.action.new")}
          />
        </>
      }
      ariaLabel={t("prompt.page.title")}
      compact
      leading={
        <>
          <ToolbarSearch
            ariaLabel={t("prompt.search.label")}
            onChange={(value) => onQueryChange(value)}
            placeholder={t("prompt.search.placeholder")}
            value={query}
          />
          <ToolbarSingleSelectDropdown
            ariaLabel={t("toolbar.sort.label")}
            icon={<ArrowDownWideNarrow size={15} />}
            onChange={onSortModeChange}
            options={[
              { label: t("toolbar.sort.updatedAt"), value: "updated" },
              { label: t("toolbar.sort.createdAt"), value: "created" },
              { label: t("toolbar.sort.name"), value: "title" },
              { label: t("prompt.toolbar.sort.copyCount"), value: "copy-count" },
            ]}
            value={sortMode}
          />
          <ToolbarSortDirectionButton
            direction={sortDirection}
            label={t("toolbar.sort.direction.label")}
            onClick={() => onSortDirectionChange(sortDirection === "desc" ? "asc" : "desc")}
            title={t(sortDirection === "desc" ? "toolbar.sort.direction.descTitle" : "toolbar.sort.direction.ascTitle")}
          />
        </>
      }
      sticky
      stickyBleed
    />
  );
}

function PromptStageCard({
  actionsDisabled,
  activeIndex,
  activeNote,
  busyAction,
  copied,
  filteredCount,
  notes,
  onCopyActive,
  onDeleteActive,
  onNextNote,
  onOptimizeActive,
  onPreviousNote,
  onSaveActive,
  onSelectNote,
  onTranslateActive,
  translationTarget,
}: {
  actionsDisabled: boolean;
  activeIndex: number;
  activeNote: PromptNote | null;
  busyAction?: PromptAction;
  copied: boolean;
  filteredCount: number;
  notes: PromptNote[];
  onCopyActive: (text: string) => void;
  onDeleteActive: () => void;
  onNextNote: () => void;
  onOptimizeActive: (sourceText: string) => Promise<boolean>;
  onPreviousNote: () => void;
  onSaveActive: (values: PromptNoteDraft, targetFace?: PromptCardFace) => void;
  onSelectNote: (noteId: string) => void;
  onTranslateActive: () => void;
  translationTarget: string;
}) {
  const { t } = useI18n();
  const [infoOpen, setInfoOpen] = useState(false);
  const [editable, setEditable] = useState(() => !activeNote);
  const [draftContent, setDraftContent] = useState(() => activeNote?.content ?? "");
  const [cardFace, setCardFace] = useState<PromptCardFace>("front");
  const [switchDirection, setSwitchDirection] = useState<PromptSwitchDirection>("next");
  const activeBusy = Boolean(busyAction);
  const optimizedText = activeNote?.optimizedText?.trim() ?? "";
  const hasOptimizedText = optimizedText.length > 0;
  const visibleContent = cardFace === "back" ? optimizedText : activeNote?.content ?? "";
  const translated = Boolean(activeNote?.translatedText);
  const updatedAt = activeNote?.updatedAt ?? new Date().toISOString();
  const displayContent = editable ? draftContent : visibleContent;
  const characterCount = displayContent.length;
  const lineCount = displayContent ? displayContent.split("\n").length : 0;
  const canSave = draftContent.trim().length > 0;
  const copyLabel = activeNote
    ? copied
      ? t("prompt.action.copied")
      : t("prompt.action.copy")
    : t("prompt.action.copy");
  const translateLabel = activeNote && translated
    ? t("prompt.action.retranslate", { language: translationTarget })
    : t("prompt.action.translate", { language: translationTarget });
  const optimizeLabel = cardFace === "front" && hasOptimizedText
    ? t("prompt.action.showOptimized")
    : cardFace === "back"
      ? t("prompt.action.reoptimize")
      : t("prompt.action.optimize");
  const saveLabel = editable
    ? activeNote
      ? t("prompt.editDialog.submit")
      : t("prompt.composer.create")
    : t("prompt.action.edit");
  const canSwitchNotes = notes.length > 1;

  const sideCards = useMemo(() => buildPromptSwitcherCards(notes, activeIndex), [activeIndex, notes]);

  useEffect(() => {
    setDraftContent(cardFace === "back" ? activeNote?.optimizedText ?? "" : activeNote?.content ?? "");
    setEditable(!activeNote);
  }, [activeNote?.content, activeNote?.id, activeNote?.optimizedText, cardFace]);

  useEffect(() => {
    setCardFace("front");
  }, [activeNote?.id]);

  function handleSaveToggle() {
    if (!editable) {
      setEditable(true);
      return;
    }
    if (!canSave) {
      return;
    }
    onSaveActive({
      content: draftContent,
      projectPath: activeNote?.projectPath ?? "",
      sessionName: activeNote?.sessionName ?? "",
      tags: activeNote?.tags ?? [],
      title: activeNote?.title ?? "",
    }, cardFace);
    setEditable(false);
  }

  async function handleOptimizeVisibleFace() {
    if (!activeNote || activeBusy || actionsDisabled) {
      return;
    }
    if (cardFace === "front" && hasOptimizedText) {
      setEditable(false);
      setCardFace("back");
      return;
    }

    const sourceText = (cardFace === "back" ? optimizedText : activeNote.content).trim();
    if (!sourceText) {
      return;
    }

    const optimized = await onOptimizeActive(sourceText);
    if (optimized) {
      setEditable(false);
      setCardFace("back");
    }
  }

  function handlePreviousNote() {
    setSwitchDirection("previous");
    onPreviousNote();
  }

  function handleNextNote() {
    setSwitchDirection("next");
    onNextNote();
  }

  function handleSelectSwitcherNote(noteId: string, offset: number) {
    setSwitchDirection(offset < 0 ? "previous" : "next");
    onSelectNote(noteId);
  }

  function renderActiveCardFace(face: PromptCardFace, faceContent: string, faceLabel: string) {
    const activeSurface = face === cardFace;
    const faceDisplayContent = activeSurface ? displayContent : faceContent;
    const faceCharacterCount = faceDisplayContent.length;
    const faceLineCount = faceDisplayContent ? faceDisplayContent.split("\n").length : 0;
    const emptyBackFace = face === "back" && !faceContent;
    const faceTitle = face === "back" ? t("prompt.optimized.label") : activeNote?.title || t("prompt.composer.title");

    return (
      <article
        aria-hidden={!activeSurface}
        className="absolute inset-0 flex flex-col overflow-hidden rounded-[2rem] border border-theme-card-border bg-[linear-gradient(145deg,rgb(var(--theme-card-bg)/0.99),rgb(var(--theme-card-header)/0.98))] shadow-[0_38px_96px_rgb(var(--theme-panel-shadow)/0.46)] max-md:rounded-[1.5rem]"
        style={promptCardFaceStyle(face, activeSurface)}
      >
        <span className="pointer-events-none absolute inset-x-0 top-0 h-16 bg-[radial-gradient(circle_at_50%_0%,rgb(var(--theme-glow)/0.16),transparent_62%)]" />
        <span className="pointer-events-none absolute left-1/2 top-2 z-20 h-1 w-14 -translate-x-1/2 rounded-full bg-theme-control-border/65" />
        <header className="relative z-10 grid min-h-16 grid-cols-[minmax(0,1fr)_auto] items-center gap-3 border-b border-theme-card-border bg-theme-card-header/82 px-4 py-3">
          <div className="min-w-0">
            <div className="flex min-w-0 items-center gap-2">
              <span className="truncate text-body-sm font-semibold text-on-surface">
                {faceTitle}
              </span>
              <span className="shrink-0 rounded-md border border-theme-control-border bg-theme-control/70 px-1.5 py-0.5 text-code-sm text-theme-control-fg max-sm:hidden">
                {faceLabel}
              </span>
            </div>
            <div className="mt-1 flex min-w-0 items-center gap-1.5 text-code-sm text-on-surface-muted">
              <FolderOpen size={13} />
              <span className="truncate">
                {activeNote?.projectPath || t("prompt.field.noProject")}
              </span>
            </div>
          </div>
          <div className="flex shrink-0 items-center gap-1.5">
            {face === "back" ? (
              <>
                <PromptCardActionButton
                  disabled={!activeSurface}
                  icon={<RotateCw size={15} />}
                  label={t("prompt.action.showOriginal")}
                  onClick={() => {
                    setEditable(false);
                    setCardFace("front");
                  }}
                />
                <PromptCardActionButton
                  disabled={!activeSurface || !activeNote || actionsDisabled || activeBusy || emptyBackFace}
                  icon={busyAction === "optimize" ? <RefreshCw className="animate-spin" size={15} /> : <Sparkles size={15} />}
                  label={t("prompt.action.reoptimize")}
                  onClick={() => {
                    void handleOptimizeVisibleFace();
                  }}
                />
              </>
            ) : null}
            {face === "front" ? (
              <>
                <PromptCardActionButton
                  disabled={!activeSurface || !activeNote}
                  icon={<Pencil size={15} />}
                  label={t("prompt.action.editInfo")}
                  onClick={() => setInfoOpen(true)}
                />
                <PromptCardActionButton
                  disabled={!activeSurface || !activeNote || actionsDisabled || activeBusy}
                  icon={<Languages className={busyAction === "translate" ? "animate-pulse" : undefined} size={15} />}
                  label={translateLabel}
                  onClick={onTranslateActive}
                />
                <PromptCardActionButton
                  disabled={!activeSurface || !activeNote || actionsDisabled || activeBusy}
                  icon={busyAction === "optimize" ? <RefreshCw className="animate-spin" size={15} /> : hasOptimizedText ? <RotateCw size={15} /> : <Sparkles size={15} />}
                  label={optimizeLabel}
                  onClick={() => {
                    void handleOptimizeVisibleFace();
                  }}
                />
              </>
            ) : null}
            <PromptCardActionButton
              disabled={!activeSurface || !activeNote}
              icon={<Trash2 size={15} />}
              label={t("prompt.action.delete")}
              onClick={onDeleteActive}
              tone="danger"
            />
          </div>
        </header>

        <div className="relative z-10 flex min-h-0 flex-1 flex-col gap-3 overflow-hidden px-5 py-4">
          {editable && activeSurface ? (
            <textarea
              aria-label={face === "back" ? t("prompt.optimized.label") : t("prompt.composer.eyebrow")}
              className="min-h-0 flex-1 resize-none rounded-lg border border-theme-control-border bg-theme-control/45 px-3 py-2 font-mono text-[0.95rem] leading-7 text-on-surface outline-none placeholder:text-outline focus:border-primary/60"
              onChange={(event) => setDraftContent(event.currentTarget.value)}
              placeholder={face === "back" ? t("prompt.optimized.empty") : t("prompt.composer.contentPlaceholder")}
              value={draftContent}
            />
          ) : faceDisplayContent ? (
            <pre className="min-h-0 flex-1 overflow-auto whitespace-pre-wrap break-words font-mono text-[0.95rem] leading-7 text-on-surface-variant">
              {faceDisplayContent}
            </pre>
          ) : (
            <button
              className="grid min-h-0 flex-1 place-items-center rounded-xl border border-dashed border-theme-card-border bg-theme-control/35 px-4 text-center text-body-sm text-on-surface-muted transition-colors hover:border-primary/45 hover:bg-theme-control/60 hover:text-on-surface disabled:pointer-events-none"
              disabled={!activeSurface}
              onClick={() => setEditable(true)}
              type="button"
            >
              {face === "back" ? t("prompt.optimized.empty") : t("prompt.empty.description")}
            </button>
          )}
          {face === "front" && activeNote?.translatedText ? (
            <div className="max-h-24 overflow-auto rounded-lg border border-theme-control-border bg-theme-control/70 px-3 py-2">
              <div className="mb-1 text-label-caps uppercase text-outline">
                {t("prompt.translation.result", { language: translationTarget })}
              </div>
              <pre className="whitespace-pre-wrap break-words text-code-sm leading-5 text-on-surface">
                <code>{activeNote.translatedText}</code>
              </pre>
            </div>
          ) : null}
        </div>

        <div className="relative z-10 flex min-h-11 items-center justify-between gap-3 border-t border-theme-card-border bg-theme-card-header/45 px-5 py-3 text-code-sm text-on-surface-muted">
          <span className="min-w-0 truncate">
            {faceCharacterCount} chars · {faceLineCount} lines
          </span>
          <span className="shrink-0">{t("prompt.copy.count", { count: activeNote?.copyCount ?? 0 })}</span>
        </div>

        <footer className="relative z-10 grid grid-cols-2 border-t border-theme-card-border bg-theme-toolbar/95">
          <button
            aria-label={copyLabel}
            className="inline-flex h-12 items-center justify-center gap-2 border-r border-theme-card-border text-body-sm font-semibold text-theme-control-fg transition-colors hover:bg-theme-control-hover hover:text-on-surface disabled:cursor-not-allowed disabled:opacity-45"
            disabled={!activeSurface || !activeNote || !faceDisplayContent}
            onClick={() => onCopyActive(faceDisplayContent)}
            type="button"
          >
            {copied ? <Check size={16} /> : <Copy size={16} />}
            <span>{copied ? t("prompt.action.copied") : t("prompt.action.copy")}</span>
          </button>
          <button
            aria-label={saveLabel}
            aria-pressed={editable && activeSurface}
            className="inline-flex h-12 items-center justify-center gap-2 text-body-sm font-semibold text-theme-control-fg transition-colors hover:bg-theme-control-hover hover:text-on-surface disabled:cursor-not-allowed disabled:opacity-45"
            disabled={!activeSurface || (editable && !canSave)}
            onClick={handleSaveToggle}
            type="button"
          >
            {editable && activeSurface ? <Check size={16} /> : <Pencil size={16} />}
            <span>{saveLabel}</span>
          </button>
        </footer>
      </article>
    );
  }

  return (
    <div className="relative mx-auto h-[38rem] w-full max-w-7xl overflow-visible max-[1359px]:h-[31rem] max-lg:h-[29rem] max-md:h-[27rem]">
      <div className="pointer-events-none absolute inset-x-0 bottom-6 top-5 rounded-[2.5rem] border border-theme-card-border/45 bg-[radial-gradient(circle_at_50%_6%,rgb(var(--theme-glow)/0.12),transparent_34%),linear-gradient(180deg,rgb(var(--theme-card-bg)/0.32),rgb(var(--theme-card-header)/0.12)_48%,transparent_82%)] shadow-[inset_0_1px_0_rgb(var(--theme-inset-highlight)/0.12)]" />
      <div className="pointer-events-none absolute left-1/2 top-12 h-[27.5rem] w-[min(78rem,calc(100vw-3rem))] -translate-x-1/2 rounded-[2.75rem] border border-theme-control-border/25 max-[1359px]:left-4 max-[1359px]:right-4 max-[1359px]:w-auto max-[1359px]:translate-x-0" />
      <div className="pointer-events-none absolute left-1/2 top-[5.2rem] h-[24.5rem] w-[min(54rem,calc(100vw-4rem))] -translate-x-1/2 rounded-[2.25rem] border border-theme-control-border/18 bg-theme-card-header/20 blur-[0.2px]" />
      <div className="pointer-events-none absolute inset-x-12 bottom-16 h-px bg-[linear-gradient(90deg,transparent,rgb(var(--theme-control-border)/0.52),transparent)] max-lg:inset-x-6" />
      <div className="pointer-events-none absolute bottom-8 left-1/2 flex -translate-x-1/2 items-center gap-1.5">
        {notes.slice(0, 7).map((note, index) => (
          <span
            className={clsx(
              "h-1.5 rounded-full border border-theme-control-border/55 transition-all duration-300",
              note.id === activeNote?.id ? "w-7 bg-primary/70" : "w-2.5 bg-theme-control/80",
            )}
            key={note.id}
            style={{ transitionDelay: `${index * 18}ms` }}
          />
        ))}
      </div>

      <div className="absolute inset-x-0 top-5 h-[33.5rem] overflow-visible [perspective:2400px] [transform-style:preserve-3d] max-[1359px]:h-[28rem] max-lg:h-[26rem]">
        {sideCards.map(({ note, offset }) => (
          <button
            aria-label={note.title || previewText(note.content) || t("prompt.list.empty")}
            className={clsx(
              "prompt-side-card-in pointer-events-auto absolute left-1/2 top-[7.75rem] hidden h-[13.75rem] w-[9.75rem] overflow-hidden rounded-[1.25rem] border border-theme-card-border bg-theme-card/58 px-3 py-3 text-left text-on-surface-variant shadow-[0_22px_58px_rgb(var(--theme-panel-shadow)/0.2)] backdrop-blur transition-[transform,opacity,filter,border-color,background-color] duration-500 ease-[cubic-bezier(.2,.8,.2,1)] hover:border-theme-nav-active-border hover:bg-theme-card/88 hover:text-on-surface lg:top-[7.35rem] lg:h-[15rem] lg:w-[10.75rem] min-[1360px]:top-[5.25rem] min-[1360px]:h-[23.5rem] min-[1360px]:w-[16.5rem] min-[1360px]:rounded-[2rem] min-[1360px]:px-4 min-[1360px]:py-4",
              Math.abs(offset) === 1 ? "min-[680px]:grid" : "min-[1360px]:grid",
            )}
            key={`${note.id}-${offset}`}
            onClick={() => handleSelectSwitcherNote(note.id, offset)}
            style={promptSideCardStyle(offset)}
            type="button"
          >
            <span className="pointer-events-none absolute inset-2 rounded-[1.25rem] border border-theme-control-border/42 min-[1360px]:rounded-[1.55rem]" />
            <span className="pointer-events-none absolute left-1/2 top-3 h-1 w-10 -translate-x-1/2 rounded-full bg-theme-control-border/70 min-[1360px]:w-12" />
            <span className="pointer-events-none absolute inset-x-0 bottom-0 h-24 bg-[linear-gradient(0deg,rgb(var(--theme-card-header)/0.92),transparent)]" />
            <span className="relative z-10 grid h-full grid-rows-[auto_minmax(0,1fr)_auto] gap-3 pt-5 min-[1360px]:gap-4">
              <span className="min-w-0">
                <span className="block truncate text-body-sm font-semibold text-on-surface">
                  {note.title || t("prompt.note.untitled")}
                </span>
                <span className="mt-1 block text-code-sm text-on-surface-muted">
                  {formatDateTime(note.updatedAt)}
                </span>
              </span>
              <span className="line-clamp-5 min-h-0 font-mono text-code-sm leading-5 min-[1360px]:line-clamp-6 min-[1360px]:text-body-sm min-[1360px]:leading-6">{note.content || t("prompt.list.empty")}</span>
              <span className="flex min-w-0 items-center gap-1 text-code-sm text-on-surface-muted">
                <FolderOpen size={13} />
                <span className="truncate">{note.projectPath || t("prompt.field.noProject")}</span>
              </span>
            </span>
          </button>
        ))}

        <button
          aria-label="Previous prompt card"
          className="absolute left-1/2 top-[15.75rem] z-50 hidden size-10 -translate-x-[24.5rem] place-items-center rounded-full border border-theme-control-border bg-theme-control/92 text-theme-control-fg shadow-[0_14px_34px_rgb(var(--theme-panel-shadow)/0.24)] transition-[transform,background-color,color] duration-200 hover:-translate-x-[24.75rem] hover:bg-theme-control-hover hover:text-on-surface disabled:cursor-not-allowed disabled:opacity-45 min-[1360px]:grid"
          disabled={!canSwitchNotes}
          onClick={handlePreviousNote}
          type="button"
        >
          <ChevronLeft size={17} />
        </button>
        <button
          aria-label="Next prompt card"
          className="absolute left-1/2 top-[15.75rem] z-50 hidden size-10 translate-x-[22rem] place-items-center rounded-full border border-theme-control-border bg-theme-control/92 text-theme-control-fg shadow-[0_14px_34px_rgb(var(--theme-panel-shadow)/0.24)] transition-[transform,background-color,color] duration-200 hover:translate-x-[22.25rem] hover:bg-theme-control-hover hover:text-on-surface disabled:cursor-not-allowed disabled:opacity-45 min-[1360px]:grid"
          disabled={!canSwitchNotes}
          onClick={handleNextNote}
          type="button"
        >
          <ChevronRight size={17} />
        </button>

        <div className="absolute left-1/2 top-0 z-50 flex w-[min(36rem,calc(100vw-3rem))] -translate-x-1/2 flex-col items-center gap-2 max-lg:w-[min(30rem,calc(100vw-3rem))] max-md:w-[calc(100vw-4rem)]">
          <div className="flex h-8 w-full items-center justify-between gap-3 px-1 text-code-sm text-on-surface-muted">
            <span className="inline-flex min-w-0 items-center gap-1.5">
              <Clock size={14} />
              <span className="truncate">{formatDateTime(updatedAt)}</span>
            </span>
            <span className="inline-flex shrink-0 items-center gap-1 rounded-lg border border-theme-control-border bg-theme-control/92 p-1 shadow-[0_10px_26px_rgb(var(--theme-panel-shadow)/0.2)]">
              <button
                aria-label="Previous prompt card"
                className="grid size-7 place-items-center rounded-md text-theme-control-fg transition-colors hover:bg-theme-control-hover hover:text-on-surface disabled:cursor-not-allowed disabled:opacity-35"
                disabled={!canSwitchNotes}
                onClick={handlePreviousNote}
                type="button"
              >
                <ChevronLeft size={15} />
              </button>
              <span className="min-w-12 px-1 text-center">
                {filteredCount === 0 ? "0 / 0" : `${Math.max(activeIndex + 1, 1)} / ${filteredCount}`}
              </span>
              <button
                aria-label="Next prompt card"
                className="grid size-7 place-items-center rounded-md text-theme-control-fg transition-colors hover:bg-theme-control-hover hover:text-on-surface disabled:cursor-not-allowed disabled:opacity-35"
                disabled={!canSwitchNotes}
                onClick={handleNextNote}
                type="button"
              >
                <ChevronRight size={15} />
              </button>
            </span>
          </div>

          <div
            className="prompt-active-card-in relative h-[28.5rem] w-full [perspective:1800px] transition-[transform] duration-500 ease-[cubic-bezier(.2,.8,.2,1)] [transform:translateZ(132px)] max-[1359px]:h-[23rem] max-lg:h-[22rem]"
            data-testid="prompt-active-card"
            key={activeNote?.id ?? "new-prompt-card"}
            style={promptActiveCardStyle(switchDirection)}
          >
            <div
              className="relative h-full w-full transition-transform duration-700 ease-[cubic-bezier(.2,.8,.2,1)]"
              style={promptCardRotatorStyle(cardFace)}
            >
              {renderActiveCardFace("front", activeNote?.content ?? "", t("prompt.original.label"))}
              {renderActiveCardFace("back", activeNote?.optimizedText ?? "", t("prompt.optimized.label"))}
            </div>
          </div>
        </div>
      </div>

      {infoOpen && activeNote ? (
        <PromptInfoDialog
          note={activeNote}
          onClose={() => setInfoOpen(false)}
          onSubmit={(values) => {
            onSaveActive(values);
            setInfoOpen(false);
          }}
        />
      ) : null}
    </div>
  );
}

function PromptInfoDialog({
  note,
  onClose,
  onSubmit,
}: {
  note: PromptNote;
  onClose: () => void;
  onSubmit: (values: PromptNoteDraft) => void;
}) {
  const { t } = useI18n();
  const [draft, setDraft] = useState(() => ({
    projectPath: note.projectPath,
    sessionName: note.sessionName,
    tagInput: note.tags.join(", "),
  }));

  function updateDraft<Key extends keyof typeof draft>(key: Key, value: (typeof draft)[Key]) {
    setDraft((current) => ({ ...current, [key]: value }));
  }

  return (
    <DialogFrame
      icon={<Pencil size={18} />}
      onClose={onClose}
      size="md"
      title={t("prompt.editDialog.title")}
      footer={
        <button
          className="inline-flex h-9 items-center justify-center gap-2 rounded-lg px-3 text-body-sm font-semibold text-theme-button-primary-fg transition-all theme-primary-gradient"
          onClick={() =>
            onSubmit({
              content: note.content,
              projectPath: draft.projectPath,
              sessionName: draft.sessionName,
              tags: parseTags(draft.tagInput),
              title: note.title,
            })
          }
          type="button"
        >
          <Check size={15} />
          <span>{t("prompt.editDialog.submit")}</span>
        </button>
      }
    >
      <div className="grid gap-4">
        <div className="grid gap-3 md:grid-cols-2">
          <label className="grid min-w-0 gap-1.5">
            <span className="text-label-caps uppercase text-outline">{t("prompt.field.projectPath")}</span>
            <input
              className="h-9 min-w-0 rounded-lg border border-theme-control-border bg-theme-control/70 px-3 text-code-sm text-on-surface outline-none placeholder:text-outline focus:border-primary/60"
              onChange={(event) => updateDraft("projectPath", event.currentTarget.value)}
              placeholder={t("prompt.composer.projectPathPlaceholder")}
              value={draft.projectPath}
            />
          </label>
          <label className="grid min-w-0 gap-1.5">
            <span className="text-label-caps uppercase text-outline">{t("prompt.field.session")}</span>
            <input
              className="h-9 min-w-0 rounded-lg border border-theme-control-border bg-theme-control/70 px-3 text-code-sm text-on-surface outline-none placeholder:text-outline focus:border-primary/60"
              onChange={(event) => updateDraft("sessionName", event.currentTarget.value)}
              placeholder={t("prompt.composer.sessionPlaceholder")}
              value={draft.sessionName}
            />
          </label>
        </div>
        <label className="grid gap-1.5">
          <span className="text-label-caps uppercase text-outline">{t("prompt.field.tags")}</span>
          <input
            className="h-9 min-w-0 rounded-lg border border-theme-control-border bg-theme-control/70 px-3 text-body-sm text-on-surface outline-none placeholder:text-outline focus:border-primary/60"
            onChange={(event) => updateDraft("tagInput", event.currentTarget.value)}
            placeholder={t("prompt.composer.tagsPlaceholder")}
            value={draft.tagInput}
          />
        </label>
      </div>
    </DialogFrame>
  );
}

function buildPromptSwitcherCards(notes: PromptNote[], activeIndex: number) {
  if (notes.length < 2 || activeIndex < 0) {
    return [];
  }

  const cards: Array<{ note: PromptNote; offset: number }> = [];
  const seenNoteIds = new Set<string>();
  for (const offset of [-2, -1, 1, 2]) {
    const index = (activeIndex + offset + notes.length) % notes.length;
    const note = notes[index];
    if (!note || note.id === notes[activeIndex]?.id || seenNoteIds.has(note.id)) {
      continue;
    }

    seenNoteIds.add(note.id);
    cards.push({ note, offset });
  }

  return cards;
}

function promptSideCardStyle(offset: number) {
  const direction = Math.sign(offset);
  const distance = Math.abs(offset);
  const offsetX = direction < 0
    ? `calc(-1 * ${distance === 1 ? "clamp(24rem, 42vw, 31rem)" : "clamp(28rem, 50vw, 40rem)"})`
    : distance === 1
      ? "clamp(24rem, 42vw, 31rem)"
      : "clamp(28rem, 50vw, 40rem)";
  const offsetY = distance === 1 ? 130 : 108;
  const rotateY = direction * (distance === 1 ? -50 : -62);
  const rotateZ = direction * (distance === 1 ? 2.5 : 5);
  const scale = distance === 1 ? "0.66" : "0.72";
  const opacity = distance === 1 ? 0.52 : 0.3;
  const filter = distance === 1 ? "saturate(0.78)" : "saturate(0.64) blur(0.4px)";

  return {
    "--prompt-side-delay": `${(distance - 1) * 52}ms`,
    "--prompt-side-enter-x": `${direction * 72}px`,
    "--prompt-side-filter": filter,
    "--prompt-side-opacity": String(opacity),
    "--prompt-side-rotate-y": `${rotateY}deg`,
    "--prompt-side-rotate-z": `${rotateZ}deg`,
    "--prompt-side-scale": scale,
    "--prompt-side-x": `calc(-50% + ${offsetX})`,
    "--prompt-side-y": `${offsetY}px`,
    filter: "var(--prompt-side-filter)",
    opacity: "var(--prompt-side-opacity)",
    transform: [
      "translateX(var(--prompt-side-x))",
      "translateY(var(--prompt-side-y))",
      "rotateY(var(--prompt-side-rotate-y))",
      "rotateZ(var(--prompt-side-rotate-z))",
      "scale(var(--prompt-side-scale))",
    ].join(" "),
    transformOrigin: direction < 0 ? "right center" : "left center",
    zIndex: 4 - distance,
  } as CSSProperties;
}

function promptActiveCardStyle(direction: PromptSwitchDirection) {
  return {
    "--prompt-active-from-rotate": direction === "next" ? "-2deg" : "2deg",
    "--prompt-active-from-x": direction === "next" ? "38px" : "-38px",
  } as CSSProperties;
}

function promptCardRotatorStyle(face: PromptCardFace) {
  return {
    transform: face === "back" ? "rotateY(180deg)" : "rotateY(0deg)",
    transformStyle: "preserve-3d",
  } as CSSProperties;
}

function promptCardFaceStyle(face: PromptCardFace, active: boolean) {
  return {
    backfaceVisibility: "hidden",
    pointerEvents: active ? "auto" : "none",
    transform: face === "back" ? "rotateY(180deg)" : "rotateY(0deg)",
    WebkitBackfaceVisibility: "hidden",
  } as CSSProperties;
}

function PromptCardActionButton({
  disabled = false,
  icon,
  label,
  onClick,
  showLabel = false,
  tone = "default",
}: {
  disabled?: boolean;
  icon: ReactNode;
  label: string;
  onClick: () => void;
  showLabel?: boolean;
  tone?: "default" | "danger";
}) {
  return (
    <button
      aria-label={label}
      className={clsx(
        "inline-flex h-8 items-center justify-center rounded-lg border text-body-sm transition-colors disabled:cursor-not-allowed disabled:opacity-45",
        showLabel ? "gap-1.5 px-2" : "w-8 px-0",
        tone === "danger"
          ? "border-status-remove/35 bg-status-remove/10 text-status-remove hover:bg-status-remove/15"
          : "border-theme-control-border bg-theme-control/80 text-theme-control-fg hover:bg-theme-control-hover hover:text-on-surface",
      )}
      disabled={disabled}
      onClick={onClick}
      title={label}
      type="button"
    >
      {icon}
      <span className={showLabel ? undefined : "sr-only"}>{label}</span>
    </button>
  );
}

function sortPromptNotes(notes: PromptNote[], sortMode: PromptSortMode, sortDirection: "asc" | "desc") {
  return [...notes].sort((first, second) => {
    const direction = sortDirection === "asc" ? 1 : -1;
    let primary = 0;

    if (sortMode === "copy-count") {
      primary = first.copyCount - second.copyCount;
    } else if (sortMode === "created") {
      primary = compareTimeAsc(first.createdAt, second.createdAt);
    } else if (sortMode === "title") {
      primary = first.title.localeCompare(second.title);
    } else {
      primary = compareTimeAsc(first.updatedAt, second.updatedAt);
    }

    if (primary !== 0) {
      return primary * direction;
    }

    return first.title.localeCompare(second.title) || first.id.localeCompare(second.id);
  });
}

function compareTimeAsc(first: string, second: string) {
  return new Date(first).getTime() - new Date(second).getTime();
}

export function readPromptNotes(): PromptNote[] {
  try {
    if (typeof localStorage === "undefined") {
      return [];
    }

    const stored = localStorage.getItem(STORAGE_KEY);
    if (!stored) {
      return [];
    }

    const parsed = JSON.parse(stored);
    return Array.isArray(parsed) ? parsed.map(normalizePromptNote).filter((note): note is PromptNote => Boolean(note)) : [];
  } catch {
    return [];
  }
}

function writePromptNotes(notes: PromptNote[]) {
  try {
    if (typeof localStorage !== "undefined") {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(notes));
    }
  } catch {
    // Local prompt notes are opportunistic until they move into the Engine store.
  }
}

function normalizePromptNote(value: unknown): PromptNote | null {
  if (!value || typeof value !== "object") {
    return null;
  }

  const candidate = value as Partial<PromptNote>;
  if (typeof candidate.id !== "string" || typeof candidate.content !== "string") {
    return null;
  }

  const now = new Date().toISOString();
  return {
    content: candidate.content,
    copyCount: normalizeCopyCount(candidate.copyCount),
    createdAt: typeof candidate.createdAt === "string" ? candidate.createdAt : now,
    id: candidate.id,
    lastCopiedAt: typeof candidate.lastCopiedAt === "string" ? candidate.lastCopiedAt : undefined,
    optimizedText: typeof candidate.optimizedText === "string" ? candidate.optimizedText : undefined,
    projectPath: typeof candidate.projectPath === "string" ? candidate.projectPath : "",
    sessionName: typeof candidate.sessionName === "string" ? candidate.sessionName : "",
    tags: Array.isArray(candidate.tags) ? candidate.tags.filter((tag): tag is string => typeof tag === "string") : [],
    title: typeof candidate.title === "string" && candidate.title.trim() ? candidate.title : "Untitled prompt",
    translatedText: typeof candidate.translatedText === "string" ? candidate.translatedText : undefined,
    updatedAt: typeof candidate.updatedAt === "string" ? candidate.updatedAt : now,
  };
}

function normalizeCopyCount(value: unknown) {
  return typeof value === "number" && Number.isFinite(value) && value > 0 ? Math.floor(value) : 0;
}

export function parseTags(value: string): string[] {
  return [...new Set(value.split(/[,，\s]+/).map((tag) => tag.trim()).filter(Boolean))];
}

function createPromptNoteId() {
  if (typeof crypto !== "undefined" && "randomUUID" in crypto) {
    return crypto.randomUUID();
  }

  return `prompt-${Date.now()}-${Math.random().toString(16).slice(2)}`;
}

function previewText(value: string) {
  const normalized = value.replace(/\s+/g, " ").trim();
  const preview = normalized.length > 88 ? `${normalized.slice(0, 88)}...` : normalized;
  return `"${preview}"`;
}

function formatDateTime(value: string) {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) {
    return value;
  }

  return new Intl.DateTimeFormat(undefined, {
    dateStyle: "medium",
    timeStyle: "short",
  }).format(date);
}

function errorMessage(error: unknown) {
  return error instanceof Error ? error.message : String(error);
}
