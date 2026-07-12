import clsx from "clsx";
import {
  ArrowDownWideNarrow,
  Check,
  ChevronLeft,
  ChevronRight,
  Clock,
  Copy,
  FolderOpen,
  Image as ImageIcon,
  Languages,
  Lightbulb,
  Pencil,
  Plus,
  RefreshCw,
  RotateCw,
  Sparkles,
  Tag,
  Trash2,
  X,
} from "lucide-react";
import { useEffect, useMemo, useRef, useState, type ClipboardEvent, type CSSProperties, type ReactNode } from "react";
import { EmptyState } from "../../components/foundation/EmptyState";
import { DialogFrame } from "../../components/foundation/DialogFrame";
import { PageHeader } from "../../components/foundation/PageHeader";
import {
  DataToolbar,
  DebouncedToolbarSearch,
  ToolbarActionButton,
  ToolbarMultiSelectDropdown,
  ToolbarSingleSelectDropdown,
  ToolbarSortDirectionButton,
  type ToolbarSelectOption,
} from "../../components/common/DataToolbar";
import { PathPickerInput } from "../../components/common/PathPickerInput";
import { ManualHelpButton } from "../../manuals/ManualHelpButton";
import {
  checkConversationTranslationAvailability,
  translateConversationCardContent,
  type ConversationCardTranslationRequest,
  type OpencodeTranslationAvailability,
  type OpencodeTranslationResult,
} from "../../services/cardTranslation";
import { selectTargetDirectory } from "../../services/catalog";
import { copyPromptImagesToClipboard, copyPromptTextToClipboard } from "../../services/promptClipboard";
import { useI18n } from "../../i18n/I18nProvider";
import { useAppSettings } from "../../store/settings/AppSettingsProvider";
import { normalizeConversationTranslationTargetLanguage } from "../../store/settings/settingsSchema";

export interface PromptNote {
  attachments: PromptImageAttachment[];
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

export interface PromptImageAttachment {
  createdAt: string;
  dataUrl: string;
  id: string;
  mimeType: string;
  name: string;
  size: number;
}

type PromptAction = "translate" | "optimize";
type PromptCardFace = "front" | "back";
type PromptCopyStep = "images" | "text";
type PromptSortMode = "updated" | "copy-count" | "created" | "title";
type PromptSwitchDirection = "next" | "previous";
type PromptTagGroupId = string;
type TranslationAvailabilityStatus = "idle" | "checking" | "available" | "unavailable";
type PromptNoteDraft = Pick<PromptNote, "attachments" | "content" | "projectPath" | "sessionName" | "tags" | "title">;
type PromptNoteDraftCache = Pick<PromptNoteDraft, "attachments" | "content">;

const STORAGE_KEY = "assetiweave.promptNotes";
const DRAFT_STORAGE_KEY = "assetiweave.promptNoteDraft";
const COPIED_RESET_MS = 1400;
const PROMPT_IMAGE_ATTACHMENT_LIMIT = 6;
const DEFAULT_PROMPT_TAG_GROUP_ID = "__default__";
const PROMPT_TAG_LIMIT = 10;
const PROMPT_TAG_MAX_LENGTH = 20;
const PROMPT_TAG_GROUP_COLOR_PALETTE = [
  {
    pillClassName: "border-primary/45 bg-primary/10 text-primary",
    swatchClassName: "border-primary/65 bg-primary",
  },
  {
    pillClassName: "border-primary-strong/45 bg-primary-strong/10 text-primary-strong",
    swatchClassName: "border-primary-strong/65 bg-primary-strong",
  },
  {
    pillClassName: "border-status-create/35 bg-status-create/15 text-status-create",
    swatchClassName: "border-status-create/65 bg-status-create",
  },
  {
    pillClassName: "border-status-update/35 bg-status-update/15 text-status-update",
    swatchClassName: "border-status-update/65 bg-status-update",
  },
  {
    pillClassName: "border-status-conflict/35 bg-status-conflict/12 text-status-conflict",
    swatchClassName: "border-status-conflict/65 bg-status-conflict",
  },
  {
    pillClassName: "border-status-remove/40 bg-status-remove/12 text-status-remove",
    swatchClassName: "border-status-remove/65 bg-status-remove",
  },
  {
    pillClassName: "border-theme-nav-active-border/45 bg-theme-nav-active-border/12 text-theme-nav-active-border",
    swatchClassName: "border-theme-nav-active-border/65 bg-theme-nav-active-border",
  },
  {
    pillClassName: "border-theme-button-primary-hover/45 bg-theme-button-primary-hover/12 text-theme-button-primary-hover",
    swatchClassName: "border-theme-button-primary-hover/65 bg-theme-button-primary-hover",
  },
] as const;
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
const PROMPT_SEARCH_COMMIT_DELAY_MS = 700;

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
  const [newDraft, setNewDraft] = useState<PromptNoteDraftCache>(() => readPromptNoteDraft());
  const [creatingNew, setCreatingNew] = useState(() => hasPromptNoteDraftContent(newDraft));
  const [query, setQuery] = useState("");
  const [selectedNoteId, setSelectedNoteId] = useState<string | null>(null);
  const [sortMode, setSortMode] = useState<PromptSortMode>("updated");
  const [sortDirection, setSortDirection] = useState<"asc" | "desc">("desc");
  const [selectedTagGroups, setSelectedTagGroups] = useState<PromptTagGroupId[]>([]);
  const [copiedState, setCopiedState] = useState<{ noteId: string; step: PromptCopyStep } | null>(null);
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

  const defaultTagGroupLabel = t("prompt.tags.default");
  const promptTagLibrary = useMemo(() => buildPromptTagLibrary(notes), [notes]);
  const tagGroupOptions = useMemo(
    () => buildPromptTagGroupOptions(notes, defaultTagGroupLabel),
    [defaultTagGroupLabel, notes],
  );

  useEffect(() => {
    const availableTagGroups = new Set(tagGroupOptions.map((option) => option.value));
    setSelectedTagGroups((current) => {
      const next = current.filter((groupId) => availableTagGroups.has(groupId));
      return next.length === current.length ? current : next;
    });
  }, [tagGroupOptions]);

  const filteredNotes = useMemo(() => {
    const normalizedQuery = query.trim().toLowerCase();
    const selectedTagGroupSet = new Set(selectedTagGroups);
    const filtered = notes.filter((note) => {
      if (selectedTagGroupSet.size > 0 && !getPromptNoteTagGroupIds(note).some((groupId) => selectedTagGroupSet.has(groupId))) {
        return false;
      }

      if (!normalizedQuery) {
        return true;
      }

      return [
        note.content,
        note.optimizedText ?? "",
        note.projectPath,
        note.sessionName,
        note.translatedText ?? "",
        getPromptNoteTagGroupIds(note).map((groupId) => getPromptTagGroupLabel(groupId, defaultTagGroupLabel)).join(" "),
      ]
        .join(" ")
        .toLowerCase()
        .includes(normalizedQuery);
    });
    return sortPromptNotes(filtered, sortMode, sortDirection);
  }, [defaultTagGroupLabel, notes, query, selectedTagGroups, sortDirection, sortMode]);
  const translationTarget = normalizeConversationTranslationTargetLanguage(
    settings.conversationTranslation.targetLanguage,
  );
  const actionsDisabled = availability !== "available";
  const activeNote = creatingNew ? null : filteredNotes.find((note) => note.id === selectedNoteId) ?? filteredNotes[0] ?? null;
  const activeNoteIndex = activeNote ? filteredNotes.findIndex((note) => note.id === activeNote.id) : -1;

  function handleSaveNote(values: PromptNoteDraft, targetFace: PromptCardFace = "front") {
    const normalizedContent = values.content.trim();
    const attachments = normalizePromptImageAttachments(values.attachments);
    if (!normalizedContent && attachments.length === 0) {
      return;
    }

    const now = new Date().toISOString();
    if (activeNote && !creatingNew) {
      setNotes((current) =>
        current.map((note) =>
          note.id === activeNote.id
            ? {
                ...note,
                attachments: targetFace === "front" ? attachments : note.attachments,
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
      attachments,
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
    clearPromptNoteDraft();
    setNewDraft(createEmptyPromptNoteDraft());
  }

  function handleDeleteNote(noteId: string) {
    setNotes((current) => current.filter((note) => note.id !== noteId));
    setSelectedNoteId((current) => (current === noteId ? null : current));
  }

  async function handleCopyNoteStep(note: PromptNote, step: PromptCopyStep, text: string, attachments: PromptImageAttachment[]) {
    try {
      if (step === "images") {
        await copyPromptImagesToClipboard(attachments);
      } else {
        await copyPromptTextToClipboard(text);
      }
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
      setCopiedState({ noteId: note.id, step });
      copiedResetTimerRef.current = window.setTimeout(() => {
        setCopiedState((current) => (current?.noteId === note.id && current.step === step ? null : current));
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

  function handleNewDraftChange(value: PromptNoteDraftCache) {
    setNewDraft(value);
    writePromptNoteDraft(value);
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
    <section className="flex min-h-0 flex-1 flex-col gap-[var(--app-section-gap)] overflow-hidden px-[var(--app-page-x)] py-[var(--app-page-y)]">
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
        onTagGroupClear={() => setSelectedTagGroups([])}
        onTagGroupToggle={(groupId) => setSelectedTagGroups((current) => togglePromptTagGroupFilter(current, groupId))}
        query={query}
        selectedTagGroups={selectedTagGroups}
        sortDirection={sortDirection}
        sortMode={sortMode}
        tagGroupOptions={tagGroupOptions}
      />

      <div className="-mt-4 mx-auto flex min-h-0 w-full max-w-7xl flex-1 flex-col gap-2 overflow-visible">
        <PromptStageCard
          actionsDisabled={actionsDisabled}
          activeIndex={activeNoteIndex}
          activeNote={activeNote}
          busyAction={activeNote ? busyActions[activeNote.id] : undefined}
          copiedStep={activeNote && copiedState?.noteId === activeNote.id ? copiedState.step : null}
          filteredCount={filteredNotes.length}
          newDraft={newDraft}
          notes={filteredNotes}
          onCopyActive={(step, text, attachments) => {
            if (activeNote) {
              void handleCopyNoteStep(activeNote, step, text, attachments);
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
          onNewDraftChange={handleNewDraftChange}
          onPreviousNote={() => handleSelectAdjacentNote(-1)}
          onSaveActive={handleSaveNote}
          onSelectNote={handleSelectNote}
          onTranslateActive={() => {
            if (activeNote) {
              void handleTranslateNote(activeNote);
            }
          }}
          tagLibrary={promptTagLibrary}
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
  onTagGroupClear,
  onTagGroupToggle,
  query,
  selectedTagGroups,
  sortDirection,
  sortMode,
  tagGroupOptions,
}: {
  onCreateNew: () => void;
  onQueryChange: (value: string) => void;
  onSortDirectionChange: (direction: "asc" | "desc") => void;
  onSortModeChange: (mode: PromptSortMode) => void;
  onTagGroupClear: () => void;
  onTagGroupToggle: (groupId: PromptTagGroupId) => void;
  query: string;
  selectedTagGroups: PromptTagGroupId[];
  sortDirection: "asc" | "desc";
  sortMode: PromptSortMode;
  tagGroupOptions: ToolbarSelectOption<PromptTagGroupId>[];
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
          <DebouncedToolbarSearch
            ariaLabel={t("prompt.search.label")}
            commitDelayMs={PROMPT_SEARCH_COMMIT_DELAY_MS}
            onChange={onQueryChange}
            placeholder={t("prompt.search.placeholder")}
            submitLabel={t("prompt.search.submit")}
            value={query}
          />
          <ToolbarMultiSelectDropdown
            allLabel={t("prompt.tags.all")}
            ariaLabel={t("toolbar.filter.tag")}
            clearLabel={t("toolbar.filter.clear")}
            emptyLabel={t("toolbar.filter.empty")}
            icon={<Tag size={15} />}
            label={t("toolbar.filter.tag")}
            onClear={onTagGroupClear}
            onToggleValue={onTagGroupToggle}
            options={tagGroupOptions}
            selectedValues={selectedTagGroups}
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
  copiedStep,
  filteredCount,
  newDraft,
  notes,
  onCopyActive,
  onDeleteActive,
  onNextNote,
  onNewDraftChange,
  onOptimizeActive,
  onPreviousNote,
  onSaveActive,
  onSelectNote,
  onTranslateActive,
  tagLibrary,
  translationTarget,
}: {
  actionsDisabled: boolean;
  activeIndex: number;
  activeNote: PromptNote | null;
  busyAction?: PromptAction;
  copiedStep: PromptCopyStep | null;
  filteredCount: number;
  newDraft: PromptNoteDraftCache;
  notes: PromptNote[];
  onCopyActive: (step: PromptCopyStep, text: string, attachments: PromptImageAttachment[]) => void;
  onDeleteActive: () => void;
  onNextNote: () => void;
  onNewDraftChange: (value: PromptNoteDraftCache) => void;
  onOptimizeActive: (sourceText: string) => Promise<boolean>;
  onPreviousNote: () => void;
  onSaveActive: (values: PromptNoteDraft, targetFace?: PromptCardFace) => void;
  onSelectNote: (noteId: string) => void;
  onTranslateActive: () => void;
  tagLibrary: string[];
  translationTarget: string;
}) {
  const { t } = useI18n();
  const [infoOpen, setInfoOpen] = useState(false);
  const [editable, setEditable] = useState(() => !activeNote);
  const [draftContent, setDraftContent] = useState(() => activeNote?.content ?? newDraft.content);
  const [draftAttachments, setDraftAttachments] = useState<PromptImageAttachment[]>(() => activeNote?.attachments ?? newDraft.attachments);
  const [cardFace, setCardFace] = useState<PromptCardFace>("front");
  const [switchDirection, setSwitchDirection] = useState<PromptSwitchDirection>("next");
  const draftContentRef = useRef(draftContent);
  const activeBusy = Boolean(busyAction);
  const optimizedText = activeNote?.optimizedText?.trim() ?? "";
  const hasOptimizedText = optimizedText.length > 0;
  const visibleContent = cardFace === "back" ? optimizedText : activeNote?.content ?? "";
  const translated = Boolean(activeNote?.translatedText);
  const updatedAt = activeNote?.updatedAt ?? new Date().toISOString();
  const displayContent = editable ? draftContent : visibleContent;
  const characterCount = displayContent.length;
  const lineCount = displayContent ? displayContent.split("\n").length : 0;
  const canSave = draftContent.trim().length > 0 || draftAttachments.length > 0;
  const copyTextLabel = copiedStep === "text" ? t("prompt.action.copiedText") : t("prompt.action.copyText");
  const copyImagesLabel = copiedStep === "images" ? t("prompt.action.copiedImages") : t("prompt.action.copyImages");
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
    const nextDraftContent = activeNote
      ? cardFace === "back" ? activeNote.optimizedText ?? "" : activeNote.content
      : newDraft.content;
    const nextDraftAttachments = activeNote && cardFace === "front"
      ? activeNote.attachments
      : !activeNote && cardFace === "front"
        ? newDraft.attachments
        : [];
    setDraftContent(nextDraftContent);
    setDraftAttachments(nextDraftAttachments);
    setEditable(!activeNote);
  }, [activeNote?.attachments, activeNote?.content, activeNote?.id, activeNote?.optimizedText, cardFace, newDraft]);

  useEffect(() => {
    draftContentRef.current = draftContent;
  }, [draftContent]);

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
      attachments: draftAttachments,
      projectPath: activeNote?.projectPath ?? "",
      sessionName: activeNote?.sessionName ?? "",
      tags: activeNote?.tags ?? [],
      title: activeNote?.title ?? "",
    }, cardFace);
    setEditable(false);
  }

  function handleDraftContentChange(value: string) {
    draftContentRef.current = value;
    setDraftContent(value);
    if (!activeNote && cardFace === "front") {
      onNewDraftChange({
        attachments: draftAttachments,
        content: value,
      });
    }
  }

  function updateDraftAttachments(updater: (current: PromptImageAttachment[]) => PromptImageAttachment[]) {
    setDraftAttachments((current) => {
      const next = normalizePromptImageAttachments(updater(current));
      if (!activeNote && cardFace === "front") {
        onNewDraftChange({
          attachments: next,
          content: draftContentRef.current,
        });
      }
      return next;
    });
  }

  async function handlePromptImagePaste(event: ClipboardEvent<HTMLTextAreaElement>) {
    if (cardFace !== "front") {
      return;
    }

    const imageFiles = getClipboardImageFiles(event.clipboardData);
    if (imageFiles.length === 0) {
      return;
    }

    const availableSlots = Math.max(PROMPT_IMAGE_ATTACHMENT_LIMIT - draftAttachments.length, 0);
    if (availableSlots === 0) {
      return;
    }

    const nextAttachments = await Promise.all(
      imageFiles.slice(0, availableSlots).map((file) => readPromptImageAttachment(file)),
    );
    updateDraftAttachments((current) => [...current, ...nextAttachments]);
  }

  function handleRemoveDraftAttachment(attachmentId: string) {
    updateDraftAttachments((current) => current.filter((attachment) => attachment.id !== attachmentId));
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
    const faceAttachments = face === "front"
      ? activeSurface
        ? draftAttachments
        : activeNote?.attachments ?? []
      : [];
    const faceCharacterCount = faceDisplayContent.length;
    const faceLineCount = faceDisplayContent ? faceDisplayContent.split("\n").length : 0;
    const emptyBackFace = face === "back" && !faceContent;
    const tagGroupIds = activeNote ? getPromptNoteTagGroupIds(activeNote) : [DEFAULT_PROMPT_TAG_GROUP_ID];

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
            <div className="flex min-w-0 flex-wrap items-center gap-2">
              <PromptTagGroupPills
                defaultLabel={t("prompt.tags.default")}
                groupIds={tagGroupIds}
                maxVisible={2}
              />
              <span className="shrink-0 rounded-md border border-theme-control-border bg-theme-control/70 px-1.5 py-0.5 text-code-sm text-theme-control-fg max-sm:hidden">
                {faceLabel}
              </span>
            </div>
            <div className="mt-1 flex min-w-0 items-center gap-1.5 text-code-sm text-on-surface-muted">
              <Clock size={13} />
              <span className="truncate">
                {formatDateTime(updatedAt)}
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
              onChange={(event) => handleDraftContentChange(event.currentTarget.value)}
              onPaste={(event) => {
                void handlePromptImagePaste(event);
              }}
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
          {faceAttachments.length > 0 ? (
            <PromptImageAttachmentStrip
              attachments={faceAttachments}
              editable={editable && activeSurface && face === "front"}
              label={t("prompt.attachments.label")}
              onRemove={handleRemoveDraftAttachment}
              removeLabel={(name) => t("prompt.attachments.remove", { name })}
            />
          ) : null}
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
            {faceCharacterCount} chars · {faceLineCount} lines{faceAttachments.length > 0 ? ` · ${t("prompt.attachments.count", { count: faceAttachments.length })}` : ""}
          </span>
          <span className="shrink-0">{t("prompt.copy.count", { count: activeNote?.copyCount ?? 0 })}</span>
        </div>

        <footer className={clsx(
          "relative z-10 grid border-t border-theme-card-border bg-theme-toolbar/95",
          faceAttachments.length > 0 ? "grid-cols-3" : "grid-cols-2",
        )}>
          {faceAttachments.length > 0 ? (
            <button
              aria-label={copyImagesLabel}
              className="inline-flex h-12 min-w-0 items-center justify-center gap-2 border-r border-theme-card-border px-2 text-body-sm font-semibold text-theme-control-fg transition-colors hover:bg-theme-control-hover hover:text-on-surface disabled:cursor-not-allowed disabled:opacity-45"
              disabled={!activeSurface || !activeNote}
              onClick={() => onCopyActive("images", faceDisplayContent, faceAttachments)}
              type="button"
            >
              {copiedStep === "images" ? <Check size={16} /> : <ImageIcon size={16} />}
              <span className="grid size-5 shrink-0 place-items-center rounded-full border border-theme-card-border text-[0.7rem] leading-none text-on-surface-muted">
                1
              </span>
              <span className="truncate">{copyImagesLabel}</span>
            </button>
          ) : null}
          <button
            aria-label={copyTextLabel}
            className="inline-flex h-12 items-center justify-center gap-2 border-r border-theme-card-border text-body-sm font-semibold text-theme-control-fg transition-colors hover:bg-theme-control-hover hover:text-on-surface disabled:cursor-not-allowed disabled:opacity-45"
            disabled={!activeSurface || !activeNote || !faceDisplayContent}
            onClick={() => onCopyActive("text", faceDisplayContent, faceAttachments)}
            type="button"
          >
            {copiedStep === "text" ? <Check size={16} /> : <Copy size={16} />}
            {faceAttachments.length > 0 ? (
              <span className="grid size-5 shrink-0 place-items-center rounded-full border border-theme-card-border text-[0.7rem] leading-none text-on-surface-muted">
                2
              </span>
            ) : null}
            <span className="truncate">{copyTextLabel}</span>
          </button>
          <button
            aria-label={saveLabel}
            aria-pressed={editable && activeSurface}
            className="inline-flex h-12 min-w-0 items-center justify-center gap-2 px-2 text-body-sm font-semibold text-theme-control-fg transition-colors hover:bg-theme-control-hover hover:text-on-surface disabled:cursor-not-allowed disabled:opacity-45"
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
      <div className="pointer-events-none absolute inset-x-0 bottom-6 top-5 rounded-[2.5rem] bg-[radial-gradient(ellipse_at_50%_0%,rgb(var(--theme-glow)/0.10),transparent_52%)]" />
      <div className="pointer-events-none absolute inset-x-16 bottom-14 h-px bg-[linear-gradient(90deg,transparent,rgb(var(--theme-control-border)/0.32),transparent)] max-lg:inset-x-6" />
      <div className="pointer-events-none absolute bottom-6 left-1/2 flex -translate-x-1/2 items-center gap-2">
        {notes.slice(0, 7).map((note, index) => (
          <span
            className={clsx(
              "h-[5px] rounded-full transition-all duration-300",
              note.id === activeNote?.id
                ? "w-8 bg-primary/80 shadow-[0_0_10px_rgb(var(--theme-glow)/0.5)]"
                : "w-[5px] bg-theme-control-border/70",
            )}
            key={note.id}
            style={{ transitionDelay: `${index * 24}ms` }}
          />
        ))}
      </div>

      <div className="group/stage absolute inset-x-0 top-5 h-[33.5rem] overflow-visible [perspective:1400px] [transform-style:preserve-3d] max-[1359px]:h-[28rem] max-lg:h-[26rem]">
        {sideCards.map(({ note, offset }) => (
          <button
            aria-label={promptSwitcherCardAriaLabel(note, t("prompt.tags.default"), t("prompt.list.empty"))}
            className={clsx(
              "prompt-side-card-in pointer-events-auto absolute left-1/2 top-[5rem] hidden h-[20rem] w-[14rem] overflow-hidden rounded-[1.5rem] border border-theme-card-border/60 bg-theme-card/65 px-3 py-3 text-left text-on-surface-variant shadow-[0_18px_48px_rgb(var(--theme-panel-shadow)/0.28)] backdrop-blur-sm transition-[transform,opacity,border-color,background-color,box-shadow,filter] duration-500 ease-[cubic-bezier(.16,.84,.22,1)] hover:!translate-y-[-8px] hover:!scale-[0.92] hover:border-primary/55 hover:bg-theme-card/92 hover:text-on-surface hover:!opacity-100 hover:![filter:brightness(1)] hover:shadow-[0_28px_64px_rgb(var(--theme-panel-shadow)/0.42),0_0_24px_rgb(var(--theme-glow)/0.12)] lg:h-[22rem] lg:w-[15rem] min-[1360px]:top-[3.5rem] min-[1360px]:h-[26rem] min-[1360px]:w-[18rem] min-[1360px]:rounded-[1.75rem] min-[1360px]:px-4 min-[1360px]:py-4",
              Math.abs(offset) === 1 ? "min-[680px]:grid" : "min-[1360px]:grid",
            )}
            key={`${note.id}-${offset}`}
            onClick={() => handleSelectSwitcherNote(note.id, offset)}
            style={promptSideCardStyle(offset)}
            type="button"
          >
            <span className="pointer-events-none absolute left-1/2 top-2.5 h-[3px] w-8 -translate-x-1/2 rounded-full bg-theme-control-border/50 min-[1360px]:w-10" />
            <span className="pointer-events-none absolute inset-x-0 bottom-0 h-20 bg-[linear-gradient(0deg,rgb(var(--theme-card-header)/0.95),transparent)]" />
            <span className="relative z-10 grid h-full grid-rows-[auto_minmax(0,1fr)_auto] gap-2 pt-4 min-[1360px]:gap-3">
              <span className="min-w-0">
                <PromptTagGroupPills
                  compact
                  defaultLabel={t("prompt.tags.default")}
                  groupIds={getPromptNoteTagGroupIds(note)}
                  maxVisible={1}
                />
                <span className="mt-1 block text-code-sm text-on-surface-muted">
                  {formatDateTime(note.updatedAt)}
                </span>
              </span>
              <span className="line-clamp-6 min-h-0 font-mono text-code-sm leading-[1.45] min-[1360px]:line-clamp-8 min-[1360px]:text-body-sm min-[1360px]:leading-relaxed">{note.content || t("prompt.list.empty")}</span>
              <span className="flex min-w-0 items-center gap-1 text-code-sm text-on-surface-muted">
                {note.projectPath ? (
                  <>
                    <FolderOpen size={13} />
                    <span className="truncate">{note.projectPath}</span>
                  </>
                ) : (
                  <span className="truncate">{t("prompt.copy.count", { count: note.copyCount })}</span>
                )}
              </span>
            </span>
          </button>
        ))}

        <button
          aria-label="Previous prompt card"
          className="absolute left-1/2 top-[14.5rem] z-50 hidden size-10 -translate-x-[22.5rem] place-items-center rounded-full border border-theme-control-border/70 bg-theme-card/90 text-theme-control-fg shadow-[0_10px_28px_rgb(var(--theme-panel-shadow)/0.28)] backdrop-blur-sm transition-[transform,background-color,color,box-shadow] duration-250 hover:-translate-x-[22.75rem] hover:bg-theme-control-hover hover:text-on-surface hover:shadow-[0_12px_32px_rgb(var(--theme-panel-shadow)/0.36)] disabled:cursor-not-allowed disabled:opacity-40 min-[1360px]:grid"
          disabled={!canSwitchNotes}
          onClick={handlePreviousNote}
          type="button"
        >
          <ChevronLeft size={17} />
        </button>
        <button
          aria-label="Next prompt card"
          className="absolute left-1/2 top-[14.5rem] z-50 hidden size-10 translate-x-[20rem] place-items-center rounded-full border border-theme-control-border/70 bg-theme-card/90 text-theme-control-fg shadow-[0_10px_28px_rgb(var(--theme-panel-shadow)/0.28)] backdrop-blur-sm transition-[transform,background-color,color,box-shadow] duration-250 hover:translate-x-[20.25rem] hover:bg-theme-control-hover hover:text-on-surface hover:shadow-[0_12px_32px_rgb(var(--theme-panel-shadow)/0.36)] disabled:cursor-not-allowed disabled:opacity-40 min-[1360px]:grid"
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
            className="prompt-active-card-in relative h-[28.5rem] w-full [perspective:1800px] transition-[transform] duration-500 ease-[cubic-bezier(.16,.84,.22,1)] [transform:translateZ(60px)] max-[1359px]:h-[23rem] max-lg:h-[22rem]"
            data-testid="prompt-active-card"
            key={activeNote?.id ?? "new-prompt-card"}
            style={promptActiveCardStyle(switchDirection)}
          >
            <span className="prompt-card-glow-pulse pointer-events-none absolute -inset-px z-50 rounded-[2rem] max-md:rounded-[1.5rem]" />
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
          availableTags={tagLibrary}
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
  availableTags,
  note,
  onClose,
  onSubmit,
}: {
  availableTags: string[];
  note: PromptNote;
  onClose: () => void;
  onSubmit: (values: PromptNoteDraft) => void;
}) {
  const { t } = useI18n();
  const tagInputRef = useRef<HTMLInputElement | null>(null);
  const [draft, setDraft] = useState(() => ({
    projectPath: note.projectPath,
    sessionName: note.sessionName,
    tags: normalizeEditablePromptTags(note.tags),
  }));
  const [tagInput, setTagInput] = useState("");
  const [pickingProjectPath, setPickingProjectPath] = useState(false);
  const tagLibrary = useMemo(() => normalizePromptTagLibrary([...availableTags, ...draft.tags]), [availableTags, draft.tags]);
  const remainingTagCount = Math.max(PROMPT_TAG_LIMIT - draft.tags.length, 0);
  const normalizedTagInput = normalizeEditablePromptTag(tagInput);
  const canAddTag = remainingTagCount > 0 && normalizedTagInput.length > 0 && !draft.tags.includes(normalizedTagInput);

  function updateDraft<Key extends keyof typeof draft>(key: Key, value: (typeof draft)[Key]) {
    setDraft((current) => ({ ...current, [key]: value }));
  }

  function focusTagInput(value?: string) {
    if (value !== undefined) {
      setTagInput(value.slice(0, PROMPT_TAG_MAX_LENGTH));
    }
    tagInputRef.current?.focus();
  }

  function removeDraftTag(tag: string) {
    updateDraft("tags", draft.tags.filter((candidate) => candidate !== tag));
  }

  function editDraftTag(tag: string) {
    removeDraftTag(tag);
    focusTagInput(tag);
  }

  function bindDraftTag(tag: string) {
    if (draft.tags.includes(tag) || draft.tags.length >= PROMPT_TAG_LIMIT) {
      return;
    }

    updateDraft("tags", normalizeEditablePromptTags([...draft.tags, tag]));
  }

  function addDraftTag() {
    if (!canAddTag) return;

    updateDraft("tags", normalizeEditablePromptTags([...draft.tags, normalizedTagInput]));
    setTagInput("");
  }

  async function handlePickProjectPath() {
    setPickingProjectPath(true);
    try {
      const selected = await selectTargetDirectory(t("prompt.project.pickDirectory"));
      if (selected) {
        updateDraft("projectPath", selected);
      }
    } finally {
      setPickingProjectPath(false);
    }
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
              attachments: note.attachments,
              content: note.content,
              projectPath: draft.projectPath,
              sessionName: draft.sessionName,
              tags: draft.tags,
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
            <PathPickerInput
              aria-label={t("prompt.field.projectPath")}
              inputClassName="h-9 bg-theme-control/70 text-code-sm focus:border-primary/60"
              onChange={(event) => updateDraft("projectPath", event.currentTarget.value)}
              onPick={() => {
                void handlePickProjectPath();
              }}
              pickLabel={t("prompt.project.pickDirectory")}
              picking={pickingProjectPath}
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
        <div className="grid gap-4">
          <p className="text-body-sm text-on-surface-muted">
            {t("prompt.tags.rule", { count: PROMPT_TAG_LIMIT, length: PROMPT_TAG_MAX_LENGTH })}
          </p>
          <div className="grid gap-2" aria-label={t("prompt.tags.library")}>
            <span className="text-label-caps uppercase text-outline">{t("prompt.tags.library")}</span>
            {tagLibrary.length > 0 ? (
              <div className="flex min-w-0 flex-wrap items-center gap-2">
                {tagLibrary.map((tag) => {
                  const bound = draft.tags.includes(tag);
                  return (
                    <PromptLibraryTagChip
                      bindLabel={t("prompt.tags.bind", { tag })}
                      bound={bound}
                      boundLabel={t("prompt.tags.bound", { tag })}
                      disabled={!bound && remainingTagCount === 0}
                      key={tag}
                      label={tag}
                      onBind={() => bindDraftTag(tag)}
                    />
                  );
                })}
              </div>
            ) : (
              <div className="flex min-h-14 items-center rounded-xl border border-dashed border-theme-control-border bg-theme-control/25 px-4 text-body-sm text-outline">
                {t("prompt.tags.libraryEmpty")}
              </div>
            )}
          </div>
          <div className="grid gap-2" aria-label={t("prompt.tags.current")}>
            <span className="text-label-caps uppercase text-outline">{t("prompt.tags.current")}</span>
            {draft.tags.length > 0 ? (
              <div className="flex min-w-0 flex-wrap items-center gap-2">
                {draft.tags.map((tag) => (
                  <PromptEditableTagChip
                    editLabel={t("prompt.tags.edit", { tag })}
                    key={tag}
                    label={tag}
                    onEdit={() => editDraftTag(tag)}
                    onRemove={() => removeDraftTag(tag)}
                    removeLabel={t("prompt.tags.remove", { tag })}
                  />
                ))}
              </div>
            ) : (
              <div className="flex min-h-14 items-center rounded-xl border border-dashed border-theme-control-border bg-theme-control/25 px-4 text-body-sm text-outline">
                {t("prompt.tags.empty")}
              </div>
            )}
          </div>
          <div className="grid gap-2 sm:grid-cols-[minmax(0,1fr)_auto]">
            <input
              aria-label={t("prompt.tags.addInput")}
              className="h-11 min-w-0 rounded-xl border border-theme-control-border bg-theme-control/45 px-4 text-body-sm text-on-surface outline-none placeholder:text-outline transition-[border-color,box-shadow,background-color] focus:border-primary/75 focus:bg-theme-control/65 focus:shadow-[0_0_0_4px_rgb(var(--color-primary)/0.16)] disabled:cursor-not-allowed disabled:opacity-55"
              disabled={remainingTagCount === 0}
              maxLength={PROMPT_TAG_MAX_LENGTH}
              onChange={(event) => setTagInput(event.currentTarget.value.slice(0, PROMPT_TAG_MAX_LENGTH))}
              onKeyDown={(event) => {
                if (event.key === "Enter") {
                  event.preventDefault();
                  addDraftTag();
                }
              }}
              placeholder={t("prompt.tags.addPlaceholder", { count: remainingTagCount })}
              ref={tagInputRef}
              value={tagInput}
            />
            <button
              className="inline-flex h-11 min-w-[5.75rem] items-center justify-center gap-2 rounded-xl border border-theme-control-border bg-theme-control/45 px-4 text-body-sm font-semibold text-theme-control-fg transition-colors hover:bg-theme-control-hover hover:text-on-surface disabled:cursor-not-allowed disabled:opacity-45"
              disabled={!canAddTag}
              onClick={addDraftTag}
              type="button"
            >
              <Plus size={17} />
              <span>{t("prompt.tags.add")}</span>
            </button>
          </div>
        </div>
      </div>
    </DialogFrame>
  );
}

function PromptImageAttachmentStrip({
  attachments,
  editable,
  label,
  onRemove,
  removeLabel,
}: {
  attachments: PromptImageAttachment[];
  editable: boolean;
  label: string;
  onRemove: (attachmentId: string) => void;
  removeLabel: (name: string) => string;
}) {
  return (
    <div
      aria-label={label}
      className="flex max-h-16 gap-1.5 overflow-x-auto overflow-y-hidden rounded-lg border border-theme-control-border bg-theme-control/35 p-1.5"
    >
      {attachments.map((attachment) => (
        <figure
          className="group relative size-12 shrink-0 overflow-hidden rounded-md border border-theme-card-border bg-theme-card/70"
          key={attachment.id}
          title={attachment.name}
        >
          <img
            alt={attachment.name}
            className="size-full object-cover"
            src={attachment.dataUrl}
          />
          {editable ? (
            <button
              aria-label={removeLabel(attachment.name)}
              className="absolute right-1 top-1 grid size-6 place-items-center rounded-md border border-theme-control-border bg-theme-control/90 text-theme-control-fg opacity-0 shadow-[0_8px_18px_rgb(var(--theme-panel-shadow)/0.2)] transition-opacity hover:bg-theme-control-hover hover:text-on-surface focus:opacity-100 group-hover:opacity-100"
              onClick={() => onRemove(attachment.id)}
              type="button"
            >
              <X size={14} />
            </button>
          ) : null}
        </figure>
      ))}
    </div>
  );
}

function PromptEditableTagChip({
  editLabel,
  label,
  onEdit,
  onRemove,
  removeLabel,
}: {
  editLabel: string;
  label: string;
  onEdit: () => void;
  onRemove: () => void;
  removeLabel: string;
}) {
  return (
    <span
      className={clsx(
        "inline-flex h-9 min-w-0 max-w-full items-center gap-2 rounded-full border px-3 text-body-sm font-semibold shadow-[inset_0_1px_0_rgb(var(--theme-inset-highlight)/0.38)]",
        getPromptTagGroupColorClass(label),
      )}
      data-testid={`prompt-edit-tag-chip-${label}`}
      title={label}
    >
      <Tag className="shrink-0 opacity-75" size={15} />
      <span className="min-w-0 truncate">{label}</span>
      <button
        aria-label={editLabel}
        className="grid size-5 shrink-0 place-items-center rounded-full opacity-70 transition-opacity hover:opacity-100 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/55"
        onClick={onEdit}
        type="button"
      >
        <Pencil size={14} />
      </button>
      <button
        aria-label={removeLabel}
        className="grid size-5 shrink-0 place-items-center rounded-full opacity-70 transition-opacity hover:opacity-100 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/55"
        onClick={onRemove}
        type="button"
      >
        <X size={14} />
      </button>
    </span>
  );
}

function PromptLibraryTagChip({
  bindLabel,
  bound,
  boundLabel,
  disabled,
  label,
  onBind,
}: {
  bindLabel: string;
  bound: boolean;
  boundLabel: string;
  disabled: boolean;
  label: string;
  onBind: () => void;
}) {
  return (
    <button
      aria-disabled={bound || disabled}
      aria-label={bound ? boundLabel : bindLabel}
      aria-pressed={bound}
      className={clsx(
        "inline-flex h-8 min-w-0 max-w-full items-center gap-1.5 rounded-full border px-2.5 text-code-sm font-semibold shadow-[inset_0_1px_0_rgb(var(--theme-inset-highlight)/0.32)] transition-[background-color,border-color,opacity,transform] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-primary/55",
        getPromptTagGroupColorClass(label),
        bound ? "cursor-default ring-1 ring-current/20" : "hover:scale-[1.02] hover:opacity-95",
        disabled ? "cursor-not-allowed opacity-45" : undefined,
      )}
      disabled={disabled}
      onClick={() => {
        if (!bound) {
          onBind();
        }
      }}
      title={label}
      type="button"
    >
      {bound ? <Check className="shrink-0 opacity-75" size={13} /> : <Plus className="shrink-0 opacity-75" size={13} />}
      <span className="min-w-0 truncate">{label}</span>
    </button>
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
    ? `calc(-1 * ${distance === 1 ? "clamp(20rem, 36vw, 27rem)" : "clamp(26rem, 48vw, 38rem)"})`
    : distance === 1
      ? "clamp(20rem, 36vw, 27rem)"
      : "clamp(26rem, 48vw, 38rem)";
  const offsetY = distance === 1 ? 18 : 28;
  const rotateZ = direction * (distance === 1 ? -2 : -3.5);
  const scale = distance === 1 ? "0.88" : "0.78";
  const opacity = distance === 1 ? 0.75 : 0.42;
  const brightness = distance === 1 ? "brightness(0.92)" : "brightness(0.82)";

  return {
    "--prompt-side-delay": `${(distance - 1) * 80}ms`,
    "--prompt-side-enter-x": `${direction * 56}px`,
    "--prompt-side-enter-rotate": `${rotateZ * 1.6}deg`,
    "--prompt-side-filter": brightness,
    "--prompt-side-opacity": String(opacity),
    "--prompt-side-rotate-z": `${rotateZ}deg`,
    "--prompt-side-scale": scale,
    "--prompt-side-x": `calc(-50% + ${offsetX})`,
    "--prompt-side-y": `${offsetY}px`,
    filter: "var(--prompt-side-filter)",
    opacity: "var(--prompt-side-opacity)",
    transform: [
      "translateX(var(--prompt-side-x))",
      "translateY(var(--prompt-side-y))",
      "rotate(var(--prompt-side-rotate-z))",
      "scale(var(--prompt-side-scale))",
    ].join(" "),
    transformOrigin: direction < 0 ? "bottom right" : "bottom left",
    zIndex: 4 - distance,
  } as CSSProperties;
}

function promptActiveCardStyle(direction: PromptSwitchDirection) {
  return {
    "--prompt-active-from-rotate": direction === "next" ? "-1.5deg" : "1.5deg",
    "--prompt-active-from-x": direction === "next" ? "64px" : "-64px",
    "--prompt-active-from-scale": "0.94",
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

function PromptTagGroupPills({
  compact = false,
  defaultLabel,
  groupIds,
  maxVisible,
}: {
  compact?: boolean;
  defaultLabel: string;
  groupIds: PromptTagGroupId[];
  maxVisible: number;
}) {
  const visibleGroupIds = groupIds.slice(0, maxVisible);
  const hiddenCount = Math.max(groupIds.length - visibleGroupIds.length, 0);

  return (
    <>
      {visibleGroupIds.map((groupId) => (
        <PromptTagGroupPill
          compact={compact}
          defaultLabel={defaultLabel}
          groupId={groupId}
          key={groupId}
        />
      ))}
      {hiddenCount > 0 ? (
        <span
          className={clsx(
            "inline-flex shrink-0 items-center rounded-md border border-theme-control-border bg-theme-control/70 text-theme-control-fg",
            compact ? "px-1.5 py-0.5 text-code-sm" : "px-2 py-1 text-label-caps uppercase",
          )}
        >
          +{hiddenCount}
        </span>
      ) : null}
    </>
  );
}

function PromptTagGroupPill({
  compact = false,
  defaultLabel,
  groupId,
}: {
  compact?: boolean;
  defaultLabel: string;
  groupId: PromptTagGroupId;
}) {
  const label = getPromptTagGroupLabel(groupId, defaultLabel);

  return (
    <span
      className={clsx(
        "inline-flex min-w-0 max-w-full items-center rounded-md border font-semibold",
        getPromptTagGroupColorClass(groupId),
        compact ? "px-1.5 py-0.5 text-code-sm" : "px-2 py-1 text-label-caps uppercase",
      )}
      title={label}
    >
      <span className="min-w-0 truncate">{label}</span>
    </span>
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

function buildPromptTagGroupOptions(notes: PromptNote[], defaultLabel: string): ToolbarSelectOption<PromptTagGroupId>[] {
  const groupCounts = new Map<PromptTagGroupId, number>();
  for (const note of notes) {
    for (const groupId of getPromptNoteTagGroupIds(note)) {
      groupCounts.set(groupId, (groupCounts.get(groupId) ?? 0) + 1);
    }
  }

  return Array.from(groupCounts.keys())
    .sort((first, second) => {
      if (first === DEFAULT_PROMPT_TAG_GROUP_ID) return -1;
      if (second === DEFAULT_PROMPT_TAG_GROUP_ID) return 1;
      return getPromptTagGroupLabel(first, defaultLabel).localeCompare(getPromptTagGroupLabel(second, defaultLabel));
    })
    .map((groupId) => ({
      label: getPromptTagGroupLabel(groupId, defaultLabel),
      swatchClassName: getPromptTagGroupSwatchClass(groupId),
      value: groupId,
    }));
}

function buildPromptTagLibrary(notes: PromptNote[]) {
  return normalizePromptTagLibrary(notes.flatMap((note) => note.tags));
}

function getPromptNoteTagGroupIds(note: Pick<PromptNote, "tags">): PromptTagGroupId[] {
  const tags = parsePromptTagGroupIds(note.tags);
  return tags.length > 0 ? tags : [DEFAULT_PROMPT_TAG_GROUP_ID];
}

function parsePromptTagGroupIds(tags: string[]): PromptTagGroupId[] {
  return [...new Set(tags.map((tag) => tag.trim()).filter(Boolean))];
}

function normalizeEditablePromptTags(tags: string[]) {
  return parsePromptTagGroupIds(tags)
    .map((tag) => normalizeEditablePromptTag(tag))
    .filter(Boolean)
    .slice(0, PROMPT_TAG_LIMIT);
}

function normalizePromptTagLibrary(tags: string[]) {
  return [...new Set(tags.map((tag) => normalizeEditablePromptTag(tag)).filter(Boolean))]
    .sort((first, second) => first.localeCompare(second));
}

function normalizeEditablePromptTag(value: string) {
  return value.trim().slice(0, PROMPT_TAG_MAX_LENGTH);
}

function getPromptTagGroupLabel(groupId: PromptTagGroupId, defaultLabel: string) {
  return groupId === DEFAULT_PROMPT_TAG_GROUP_ID ? defaultLabel : groupId;
}

function getPromptTagGroupColorClass(groupId: PromptTagGroupId) {
  if (groupId === DEFAULT_PROMPT_TAG_GROUP_ID) {
    return "border-theme-control-border bg-theme-control text-on-surface-variant";
  }

  return getPromptTagGroupPalette(groupId).pillClassName;
}

function getPromptTagGroupSwatchClass(groupId: PromptTagGroupId) {
  if (groupId === DEFAULT_PROMPT_TAG_GROUP_ID) {
    return "border-theme-control-border bg-theme-control-hover";
  }

  return getPromptTagGroupPalette(groupId).swatchClassName;
}

function getPromptTagGroupPalette(groupId: PromptTagGroupId) {
  return PROMPT_TAG_GROUP_COLOR_PALETTE[hashPromptTagGroup(groupId) % PROMPT_TAG_GROUP_COLOR_PALETTE.length];
}

function hashPromptTagGroup(value: string) {
  let hash = 0;
  for (const character of value) {
    hash = (hash * 31 + character.charCodeAt(0)) >>> 0;
  }
  return hash;
}

function togglePromptTagGroupFilter(current: PromptTagGroupId[], groupId: PromptTagGroupId) {
  return current.includes(groupId)
    ? current.filter((candidate) => candidate !== groupId)
    : [...current, groupId];
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

function createEmptyPromptNoteDraft(): PromptNoteDraftCache {
  return {
    attachments: [],
    content: "",
  };
}

function hasPromptNoteDraftContent(draft: PromptNoteDraftCache) {
  return draft.content.trim().length > 0 || draft.attachments.length > 0;
}

function readPromptNoteDraft(): PromptNoteDraftCache {
  try {
    if (typeof localStorage === "undefined") {
      return createEmptyPromptNoteDraft();
    }

    const stored = localStorage.getItem(DRAFT_STORAGE_KEY);
    if (!stored) {
      return createEmptyPromptNoteDraft();
    }

    const parsed = JSON.parse(stored) as unknown;
    if (!parsed || typeof parsed !== "object") {
      return createEmptyPromptNoteDraft();
    }

    const candidate = parsed as { attachments?: unknown; content?: unknown };
    const draft = {
      attachments: normalizePromptImageAttachments(candidate.attachments),
      content: typeof candidate.content === "string" ? candidate.content : "",
    };
    return hasPromptNoteDraftContent(draft) ? draft : createEmptyPromptNoteDraft();
  } catch {
    return createEmptyPromptNoteDraft();
  }
}

function writePromptNoteDraft(draft: PromptNoteDraftCache) {
  try {
    if (typeof localStorage === "undefined") {
      return;
    }

    const normalizedDraft = {
      attachments: normalizePromptImageAttachments(draft.attachments),
      content: draft.content,
    };
    if (!hasPromptNoteDraftContent(normalizedDraft)) {
      localStorage.removeItem(DRAFT_STORAGE_KEY);
      return;
    }

    localStorage.setItem(DRAFT_STORAGE_KEY, JSON.stringify({
      ...normalizedDraft,
      updatedAt: new Date().toISOString(),
    }));
  } catch {
    // Prompt draft caching is opportunistic and should not block typing.
  }
}

function clearPromptNoteDraft() {
  try {
    if (typeof localStorage !== "undefined") {
      localStorage.removeItem(DRAFT_STORAGE_KEY);
    }
  } catch {
    // Ignore draft cache cleanup failures.
  }
}

function normalizePromptImageAttachments(value: unknown): PromptImageAttachment[] {
  if (!Array.isArray(value)) {
    return [];
  }

  return value
    .map(normalizePromptImageAttachment)
    .filter((attachment): attachment is PromptImageAttachment => Boolean(attachment))
    .slice(0, PROMPT_IMAGE_ATTACHMENT_LIMIT);
}

function normalizePromptImageAttachment(value: unknown): PromptImageAttachment | null {
  if (!value || typeof value !== "object") {
    return null;
  }

  const candidate = value as Partial<PromptImageAttachment>;
  if (typeof candidate.dataUrl !== "string" || !candidate.dataUrl.startsWith("data:image/")) {
    return null;
  }

  const inferredMimeType = candidate.dataUrl.slice(5, candidate.dataUrl.indexOf(";"));
  const mimeType = typeof candidate.mimeType === "string" && candidate.mimeType.startsWith("image/")
    ? candidate.mimeType
    : inferredMimeType.startsWith("image/")
      ? inferredMimeType
      : "image/png";
  const now = new Date().toISOString();

  return {
    createdAt: typeof candidate.createdAt === "string" ? candidate.createdAt : now,
    dataUrl: candidate.dataUrl,
    id: typeof candidate.id === "string" && candidate.id.trim() ? candidate.id : createPromptImageAttachmentId(),
    mimeType,
    name: typeof candidate.name === "string" && candidate.name.trim() ? candidate.name : "pasted-image",
    size: typeof candidate.size === "number" && Number.isFinite(candidate.size) && candidate.size > 0 ? Math.floor(candidate.size) : 0,
  };
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
    attachments: normalizePromptImageAttachments(candidate.attachments),
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

function createPromptImageAttachmentId() {
  if (typeof crypto !== "undefined" && "randomUUID" in crypto) {
    return crypto.randomUUID();
  }

  return `prompt-image-${Date.now()}-${Math.random().toString(16).slice(2)}`;
}

function getClipboardImageFiles(clipboardData: DataTransfer) {
  const files: File[] = [];
  const seen = new Set<string>();

  for (const item of Array.from(clipboardData.items ?? [])) {
    if (item.kind !== "file") {
      continue;
    }

    const file = item.getAsFile();
    if (file && isPromptImageFile(file)) {
      addUniquePromptImageFile(files, seen, file);
    }
  }

  for (const file of Array.from(clipboardData.files ?? [])) {
    if (isPromptImageFile(file)) {
      addUniquePromptImageFile(files, seen, file);
    }
  }

  return files;
}

function addUniquePromptImageFile(files: File[], seen: Set<string>, file: File) {
  const key = `${file.name}:${file.type}:${file.size}:${file.lastModified}`;
  if (seen.has(key)) {
    return;
  }

  seen.add(key);
  files.push(file);
}

function isPromptImageFile(file: File) {
  return file.type.startsWith("image/");
}

function readPromptImageAttachment(file: File): Promise<PromptImageAttachment> {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onerror = () => reject(reader.error ?? new Error("Failed to read pasted image."));
    reader.onload = () => {
      if (typeof reader.result !== "string" || !reader.result.startsWith("data:image/")) {
        reject(new Error("Pasted file is not a readable image."));
        return;
      }

      resolve({
        createdAt: new Date().toISOString(),
        dataUrl: reader.result,
        id: createPromptImageAttachmentId(),
        mimeType: file.type || reader.result.slice(5, reader.result.indexOf(";")) || "image/png",
        name: file.name || "pasted-image",
        size: file.size,
      });
    };
    reader.readAsDataURL(file);
  });
}

function previewText(value: string) {
  const normalized = value.replace(/\s+/g, " ").trim();
  const preview = normalized.length > 88 ? `${normalized.slice(0, 88)}...` : normalized;
  return `"${preview}"`;
}

function promptSwitcherCardAriaLabel(note: PromptNote, defaultLabel: string, emptyLabel: string) {
  const primaryGroupId = getPromptNoteTagGroupIds(note)[0] ?? DEFAULT_PROMPT_TAG_GROUP_ID;
  const groupLabel = getPromptTagGroupLabel(primaryGroupId, defaultLabel);
  const preview = note.content.trim() ? previewText(note.content) : emptyLabel;
  return `${groupLabel} · ${preview}`;
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
