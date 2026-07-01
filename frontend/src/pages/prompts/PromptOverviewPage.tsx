import clsx from "clsx";
import {
  ArrowDownUp,
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
  Sparkles,
  Tags,
  Trash2,
} from "lucide-react";
import { useEffect, useMemo, useRef, useState, type CSSProperties, type ReactNode } from "react";
import { EmptyState } from "../../components/foundation/EmptyState";
import { DialogFrame } from "../../components/foundation/DialogFrame";
import { PageHeader } from "../../components/foundation/PageHeader";
import {
  DataToolbar,
  ToolbarActionButton,
  ToolbarCluster,
  ToolbarSearch,
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
  translatedText?: string;
  updatedAt: string;
}

type PromptAction = "translate" | "optimize";
type PromptSortMode = "updated" | "copy-count" | "created";
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
  const [selectedTag, setSelectedTag] = useState<string | null>(null);
  const [sortMode, setSortMode] = useState<PromptSortMode>("updated");
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

  const tagStats = useMemo(() => buildTagStats(notes), [notes]);
  const totalCopies = useMemo(
    () => notes.reduce((total, note) => total + note.copyCount, 0),
    [notes],
  );
  const filteredNotes = useMemo(() => {
    const normalizedQuery = query.trim().toLowerCase();
    const filtered = notes.filter((note) => {
      if (selectedTag && !note.tags.includes(selectedTag)) {
        return false;
      }
      if (!normalizedQuery) {
        return true;
      }

      return [note.content, note.projectPath, note.sessionName, note.translatedText ?? "", note.tags.join(" ")]
        .join(" ")
        .toLowerCase()
        .includes(normalizedQuery);
    });
    return sortPromptNotes(filtered, sortMode);
  }, [notes, query, selectedTag, sortMode]);
  const translationTarget = normalizeConversationTranslationTargetLanguage(
    settings.conversationTranslation.targetLanguage,
  );
  const actionsDisabled = availability !== "available";
  const activeNote = creatingNew ? null : filteredNotes.find((note) => note.id === selectedNoteId) ?? filteredNotes[0] ?? null;
  const activeNoteIndex = activeNote ? filteredNotes.findIndex((note) => note.id === activeNote.id) : -1;

  function handleSaveNote(values: PromptNoteDraft) {
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
                content: normalizedContent,
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

  async function handleCopyNote(note: PromptNote) {
    try {
      await navigator.clipboard.writeText(note.content);
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

  async function handleOptimizeNote(note: PromptNote) {
    await runPromptAction(note, "optimize", {
      promptTemplate: OPTIMIZE_PROMPT_TEMPLATE,
      targetLanguage: settings.conversationTranslation.targetLanguage,
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
    request: Pick<ConversationCardTranslationRequest, "promptTemplate" | "targetLanguage">,
  ) {
    if (actionsDisabled) {
      return;
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
        text: note.content,
      });
      setNotes((current) =>
        current.map((candidate) => {
          if (candidate.id !== note.id) {
            return candidate;
          }

          return {
            ...candidate,
            content: action === "optimize" ? result.translated_text : candidate.content,
            translatedText: action === "translate" ? result.translated_text : candidate.translatedText,
            updatedAt: new Date().toISOString(),
          };
        }),
      );
    } catch (error) {
      setActionError(
        action === "translate"
          ? t("prompt.action.translateFailed", { message: errorMessage(error) })
          : t("prompt.action.optimizeFailed", { message: errorMessage(error) }),
      );
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
        description={t("prompt.page.description")}
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
        availability={availability}
        filteredCount={filteredNotes.length}
        notesCount={notes.length}
        onCreateNew={handleCreateNewNote}
        onQueryChange={setQuery}
        onSelectTag={setSelectedTag}
        onSortModeChange={setSortMode}
        query={query}
        selectedTag={selectedTag}
        sortMode={sortMode}
        tagStats={tagStats}
        totalCopies={totalCopies}
        translationCli={settings.conversationTranslation.cli}
        translationTarget={translationTarget}
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
          onCopyActive={() => {
            if (activeNote) {
              void handleCopyNote(activeNote);
            }
          }}
          onDeleteActive={() => {
            if (activeNote) {
              handleDeleteNote(activeNote.id);
            }
          }}
          onOptimizeActive={() => {
            if (activeNote) {
              void handleOptimizeNote(activeNote);
            }
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
  availability,
  filteredCount,
  notesCount,
  onCreateNew,
  onQueryChange,
  onSelectTag,
  onSortModeChange,
  query,
  selectedTag,
  sortMode,
  tagStats,
  totalCopies,
  translationCli,
  translationTarget,
}: {
  availability: TranslationAvailabilityStatus;
  filteredCount: number;
  notesCount: number;
  onCreateNew: () => void;
  onQueryChange: (value: string) => void;
  onSelectTag: (tag: string | null) => void;
  onSortModeChange: (mode: PromptSortMode) => void;
  query: string;
  selectedTag: string | null;
  sortMode: PromptSortMode;
  tagStats: Array<{ count: number; tag: string }>;
  totalCopies: number;
  translationCli: string;
  translationTarget: string;
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
          <ToolbarCluster ariaLabel={t("prompt.metric.notes")} className="text-code-sm">
            <span>{t("prompt.metric.notes")} {notesCount}</span>
            <span className="text-outline">/</span>
            <span>{t("prompt.metric.filtered")} {filteredCount}</span>
            <span className="text-outline">/</span>
            <span>{t("prompt.metric.copies")} {totalCopies}</span>
          </ToolbarCluster>
          <ToolbarCluster ariaLabel={t("prompt.translation.status")} className="text-code-sm">
            <span>
              {availabilityLabel(availability, t("prompt.translation.available"), t("prompt.translation.checking"), t("prompt.translation.unavailable"))}
            </span>
            <span className="rounded-md border border-theme-control-border bg-theme-panel px-2 py-0.5 text-theme-control-fg">
              {translationCli} · {translationTarget}
            </span>
          </ToolbarCluster>
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
          <ToolbarCluster ariaLabel={t("prompt.sort.label")}>
            <ArrowDownUp size={15} />
            <span className="text-label-caps uppercase text-outline">{t("prompt.sort.label")}</span>
            <select
              aria-label={t("prompt.sort.aria")}
              className="min-w-[7.5rem] border-0 bg-transparent text-body-sm text-on-surface outline-none"
              onChange={(event) => onSortModeChange(event.currentTarget.value as PromptSortMode)}
              value={sortMode}
            >
              <option value="updated">{t("prompt.sort.updated")}</option>
              <option value="copy-count">{t("prompt.sort.copyCount")}</option>
              <option value="created">{t("prompt.sort.created")}</option>
            </select>
          </ToolbarCluster>
          <ToolbarCluster ariaLabel={t("prompt.tags.groupTitle")} className="max-w-[32rem]">
            <Tags size={15} />
            <button
              aria-pressed={selectedTag === null}
              className={tagFilterButtonClass(selectedTag === null)}
              onClick={() => onSelectTag(null)}
              type="button"
            >
              {t("prompt.tags.all")}
            </button>
            {tagStats.map(({ count, tag }) => (
              <button
                aria-label={t("prompt.tags.filterAria", { count, tag })}
                aria-pressed={selectedTag === tag}
                className={tagFilterButtonClass(selectedTag === tag)}
                key={tag}
                onClick={() => onSelectTag(tag)}
                type="button"
              >
                <span>{tag}</span>
                <span className="rounded bg-theme-panel px-1 text-code-sm">{count}</span>
              </button>
            ))}
          </ToolbarCluster>
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
  onCopyActive: () => void;
  onDeleteActive: () => void;
  onNextNote: () => void;
  onOptimizeActive: () => void;
  onPreviousNote: () => void;
  onSaveActive: (values: PromptNoteDraft) => void;
  onSelectNote: (noteId: string) => void;
  onTranslateActive: () => void;
  translationTarget: string;
}) {
  const { t } = useI18n();
  const [infoOpen, setInfoOpen] = useState(false);
  const [editable, setEditable] = useState(() => !activeNote);
  const [draftContent, setDraftContent] = useState(() => activeNote?.content ?? "");
  const [switchDirection, setSwitchDirection] = useState<PromptSwitchDirection>("next");
  const activeBusy = Boolean(busyAction);
  const translated = Boolean(activeNote?.translatedText);
  const updatedAt = activeNote?.updatedAt ?? new Date().toISOString();
  const displayContent = editable ? draftContent : activeNote?.content ?? "";
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
  const saveLabel = editable
    ? activeNote
      ? t("prompt.editDialog.submit")
      : t("prompt.composer.create")
    : t("prompt.action.edit");
  const canSwitchNotes = notes.length > 1;

  const sideCards = useMemo(() => buildPromptSwitcherCards(notes, activeIndex), [activeIndex, notes]);

  useEffect(() => {
    setDraftContent(activeNote?.content ?? "");
    setEditable(!activeNote);
  }, [activeNote?.content, activeNote?.id]);

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
    });
    setEditable(false);
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
              "prompt-side-card-in absolute left-1/2 top-[5.25rem] hidden h-[23.5rem] w-[16.5rem] overflow-hidden rounded-[2rem] border border-theme-card-border bg-theme-card/70 px-4 py-4 text-left text-on-surface-variant shadow-[0_28px_74px_rgb(var(--theme-panel-shadow)/0.34)] backdrop-blur transition-[transform,opacity,filter,border-color,background-color] duration-500 ease-[cubic-bezier(.2,.8,.2,1)] hover:border-theme-nav-active-border hover:bg-theme-card/92 hover:text-on-surface min-[1360px]:grid",
            )}
            key={`${note.id}-${offset}`}
            onClick={() => handleSelectSwitcherNote(note.id, offset)}
            style={promptSideCardStyle(offset)}
            type="button"
          >
            <span className="pointer-events-none absolute inset-2 rounded-[1.55rem] border border-theme-control-border/42" />
            <span className="pointer-events-none absolute left-1/2 top-3 h-1 w-12 -translate-x-1/2 rounded-full bg-theme-control-border/70" />
            <span className="pointer-events-none absolute inset-x-0 bottom-0 h-24 bg-[linear-gradient(0deg,rgb(var(--theme-card-header)/0.92),transparent)]" />
            <span className="relative z-10 grid h-full grid-rows-[auto_minmax(0,1fr)_auto] gap-4 pt-5">
              <span className="min-w-0">
                <span className="block truncate text-body-sm font-semibold text-on-surface">
                  {note.title || t("prompt.note.untitled")}
                </span>
                <span className="mt-1 block text-code-sm text-on-surface-muted">
                  {formatDateTime(note.updatedAt)}
                </span>
              </span>
              <span className="line-clamp-6 min-h-0 font-mono text-body-sm leading-6">{note.content || t("prompt.list.empty")}</span>
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

          <article
            className="prompt-active-card-in relative flex h-[28.5rem] w-full flex-col overflow-hidden rounded-[2rem] border border-theme-card-border bg-theme-card shadow-[0_38px_96px_rgb(var(--theme-panel-shadow)/0.46)] transition-[transform,box-shadow,border-color] duration-500 ease-[cubic-bezier(.2,.8,.2,1)] [transform:translateZ(132px)] max-[1359px]:h-[23rem] max-lg:h-[22rem] max-md:rounded-[1.5rem]"
            data-testid="prompt-active-card"
            key={activeNote?.id ?? "new-prompt-card"}
            style={promptActiveCardStyle(switchDirection)}
          >
            <span className="pointer-events-none absolute inset-x-0 top-0 h-16 bg-[radial-gradient(circle_at_50%_0%,rgb(var(--theme-glow)/0.16),transparent_62%)]" />
            <span className="pointer-events-none absolute left-1/2 top-2 z-20 h-1 w-14 -translate-x-1/2 rounded-full bg-theme-control-border/65" />
            <header className="relative z-10 grid min-h-16 grid-cols-[minmax(0,1fr)_auto] items-center gap-3 border-b border-theme-card-border bg-theme-card-header/72 px-4 py-3">
              <div className="min-w-0">
                <div className="truncate text-body-sm font-semibold text-on-surface">
                  {activeNote?.title || t("prompt.composer.title")}
                </div>
                <div className="mt-1 flex min-w-0 items-center gap-1.5 text-code-sm text-on-surface-muted">
                  <FolderOpen size={13} />
                  <span className="truncate">
                    {activeNote?.projectPath || t("prompt.field.noProject")}
                  </span>
                </div>
              </div>
              <div className="flex shrink-0 items-center gap-1.5">
                <PromptCardActionButton
                  disabled={!activeNote}
                  icon={<Pencil size={15} />}
                  label={t("prompt.action.editInfo")}
                  onClick={() => setInfoOpen(true)}
                />
                <PromptCardActionButton
                  disabled={!activeNote || actionsDisabled || activeBusy}
                  icon={<Languages className={busyAction === "translate" ? "animate-pulse" : undefined} size={15} />}
                  label={translateLabel}
                  onClick={onTranslateActive}
                />
                <PromptCardActionButton
                  disabled={!activeNote || actionsDisabled || activeBusy}
                  icon={busyAction === "optimize" ? <RefreshCw className="animate-spin" size={15} /> : <Sparkles size={15} />}
                  label={t("prompt.action.optimize")}
                  onClick={onOptimizeActive}
                />
                <PromptCardActionButton
                  disabled={!activeNote}
                  icon={<Trash2 size={15} />}
                  label={t("prompt.action.delete")}
                  onClick={onDeleteActive}
                  tone="danger"
                />
              </div>
            </header>

            <div className="relative z-10 flex min-h-0 flex-1 flex-col gap-3 overflow-hidden px-5 py-4">
              {editable ? (
                <textarea
                  aria-label={t("prompt.composer.eyebrow")}
                  className="min-h-0 flex-1 resize-none rounded-lg border border-theme-control-border bg-theme-control/45 px-3 py-2 font-mono text-[0.95rem] leading-7 text-on-surface outline-none placeholder:text-outline focus:border-primary/60"
                  onChange={(event) => setDraftContent(event.currentTarget.value)}
                  placeholder={t("prompt.composer.contentPlaceholder")}
                  value={draftContent}
                />
              ) : displayContent ? (
                <pre className="min-h-0 flex-1 overflow-auto whitespace-pre-wrap break-words font-mono text-[0.95rem] leading-7 text-on-surface-variant">
                  {displayContent}
                </pre>
              ) : (
                <button
                  className="grid min-h-0 flex-1 place-items-center rounded-xl border border-dashed border-theme-card-border bg-theme-control/35 px-4 text-center text-body-sm text-on-surface-muted transition-colors hover:border-primary/45 hover:bg-theme-control/60 hover:text-on-surface"
                  onClick={() => setEditable(true)}
                  type="button"
                >
                  {t("prompt.empty.description")}
                </button>
              )}
              {activeNote?.translatedText ? (
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
                {characterCount} chars · {lineCount} lines
              </span>
              <span className="shrink-0">{t("prompt.copy.count", { count: activeNote?.copyCount ?? 0 })}</span>
            </div>

            <footer className="relative z-10 grid grid-cols-2 border-t border-theme-card-border bg-theme-toolbar/95">
              <button
                aria-label={copyLabel}
                className="inline-flex h-12 items-center justify-center gap-2 border-r border-theme-card-border text-body-sm font-semibold text-theme-control-fg transition-colors hover:bg-theme-control-hover hover:text-on-surface disabled:cursor-not-allowed disabled:opacity-45"
                disabled={!activeNote}
                onClick={onCopyActive}
                type="button"
              >
                {copied ? <Check size={16} /> : <Copy size={16} />}
                <span>{copied ? t("prompt.action.copied") : t("prompt.action.copy")}</span>
              </button>
              <button
                aria-label={saveLabel}
                aria-pressed={editable}
                className="inline-flex h-12 items-center justify-center gap-2 text-body-sm font-semibold text-theme-control-fg transition-colors hover:bg-theme-control-hover hover:text-on-surface disabled:cursor-not-allowed disabled:opacity-45"
                disabled={editable && !canSave}
                onClick={handleSaveToggle}
                type="button"
              >
                {editable ? <Check size={16} /> : <Pencil size={16} />}
                <span>{saveLabel}</span>
              </button>
            </footer>
          </article>
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
  const offsetX = direction * (distance === 1 ? 432 : 560);
  const offsetY = distance === 1 ? 58 : 88;
  const rotateY = direction * (distance === 1 ? -42 : -58);
  const rotateZ = direction * (distance === 1 ? 2 : 4.5);
  const scale = distance === 1 ? 0.9 : 0.78;
  const opacity = distance === 1 ? 0.86 : 0.42;
  const filter = distance === 1 ? "saturate(0.94)" : "saturate(0.72) blur(0.3px)";

  return {
    "--prompt-side-delay": `${(distance - 1) * 52}ms`,
    "--prompt-side-enter-x": `${direction * 72}px`,
    "--prompt-side-filter": filter,
    "--prompt-side-opacity": String(opacity),
    "--prompt-side-rotate-y": `${rotateY}deg`,
    "--prompt-side-rotate-z": `${rotateZ}deg`,
    "--prompt-side-scale": String(scale),
    "--prompt-side-x": `calc(-50% + ${offsetX}px)`,
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
    zIndex: 28 - distance,
  } as CSSProperties;
}

function promptActiveCardStyle(direction: PromptSwitchDirection) {
  return {
    "--prompt-active-from-rotate": direction === "next" ? "-2deg" : "2deg",
    "--prompt-active-from-x": direction === "next" ? "38px" : "-38px",
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

function tagFilterButtonClass(active: boolean) {
  return clsx(
    "inline-flex h-8 items-center gap-1.5 rounded-lg border px-2.5 text-body-sm transition-colors",
    active
      ? "border-primary/55 bg-theme-control-hover text-on-surface"
      : "border-theme-control-border bg-theme-control/75 text-theme-control-fg hover:bg-theme-control-hover hover:text-on-surface",
  );
}

function buildTagStats(notes: PromptNote[]) {
  const counts = new Map<string, number>();
  for (const note of notes) {
    for (const tag of note.tags) {
      counts.set(tag, (counts.get(tag) ?? 0) + 1);
    }
  }

  return [...counts.entries()]
    .map(([tag, count]) => ({ count, tag }))
    .sort((first, second) => second.count - first.count || first.tag.localeCompare(second.tag));
}

function sortPromptNotes(notes: PromptNote[], sortMode: PromptSortMode) {
  return [...notes].sort((first, second) => {
    if (sortMode === "copy-count") {
      return second.copyCount - first.copyCount || compareTimeDesc(first.updatedAt, second.updatedAt);
    }
    if (sortMode === "created") {
      return compareTimeDesc(first.createdAt, second.createdAt);
    }
    return compareTimeDesc(first.updatedAt, second.updatedAt);
  });
}

function compareTimeDesc(first: string, second: string) {
  return new Date(second).getTime() - new Date(first).getTime();
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

function availabilityLabel(
  status: TranslationAvailabilityStatus,
  available: string,
  checking: string,
  unavailable: string,
) {
  if (status === "available") {
    return available;
  }
  if (status === "checking" || status === "idle") {
    return checking;
  }
  return unavailable;
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
