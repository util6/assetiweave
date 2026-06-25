import { invoke } from "@tauri-apps/api/core";
import type {
  ConversationAdapter,
  ConversationSourceKind,
  ConversationMutationResult,
  ConversationQuestionDetail,
  ConversationRecordKind,
  ConversationSearchCardType,
  ConversationSearchHit,
  ConversationSearchResult,
  ConversationSearchScope,
  ConversationSessionDetail,
  ConversationSessionListItem,
  ConversationSource,
} from "../types";

export interface ConversationSessionListParams {
  adapter_id?: string | null;
  source_id?: string | null;
  query?: string | null;
  limit?: number;
  offset?: number;
}

export interface ConversationQuestionListParams {
  session_id: string;
  query?: string | null;
  limit?: number;
  offset?: number;
}

export interface ConversationSearchParams {
  record_kind?: ConversationRecordKind;
  adapter_id?: string | null;
  source_id?: string | null;
  project_path?: string | null;
  query: string;
  content_types?: ConversationSearchCardType[];
  since?: string | null;
  until?: string | null;
  timeline?: boolean;
  limit?: number;
  offset?: number;
}

export interface ConversationExportContentFilter {
  answer: boolean;
  tool: boolean;
  command: boolean;
  code: boolean;
  result: boolean;
}

export interface ConversationAdapterManifest {
  schema_version: number;
  id: string;
  name: string;
  version: string;
  protocol_version: number;
  command: string[];
  runtime?: ConversationAdapterRuntime | null;
  capabilities: string[];
  input_kinds: ConversationSourceKind[];
}

export type ConversationAdapterRuntimeKind = "node" | "python" | "bash" | "executable";

export interface ConversationAdapterRuntime {
  type: ConversationAdapterRuntimeKind;
  entry: string;
  args?: string[];
  version?: string | null;
}

export interface ConversationAdapterValidationResult {
  valid: boolean;
  manifest_path: string;
  manifest_hash: string;
  executable_path: string;
  executable_hash: string | null;
  manifest: ConversationAdapterManifest;
  warnings: string[];
}

export interface ConversationAdapterRegisterResult {
  dry_run: boolean;
  adapter: ConversationAdapter;
  validation: ConversationAdapterValidationResult;
}

export interface ConversationSourceUpsertResult {
  dry_run: boolean;
  source: ConversationSource;
}

export interface ImportConversationSourceParams {
  config_json?: string | null;
  manifest_path: string;
  record_kind?: ConversationRecordKind;
  source_id?: string | null;
  source_kind: ConversationSourceKind;
  source_location: string;
  source_name: string;
}

export interface ImportConversationSourceResult {
  adapter: ConversationAdapter;
  source: ConversationSource;
  task: ConversationSyncTaskSnapshot;
  validation: ConversationAdapterValidationResult;
}

export type ImportConversationSourceProgress = "validating" | "source" | "sync";
export type StartConversationSync = typeof syncConversations;

export type ConversationSyncTaskStatus = "running" | "completed" | "failed";

export interface ConversationSyncTaskSnapshot {
  id: string;
  status: ConversationSyncTaskStatus;
  source_id: string | null;
  adapter_id: string | null;
  record_kind?: ConversationRecordKind | null;
  dry_run: boolean;
  started_at: string;
  finished_at: string | null;
  result: unknown | null;
  error: string | null;
}

export interface ConversationSyncSummaryCounts {
  sourceCount: number;
  changedSessionCount: number;
  skippedSessionCount: number;
  turnCount: number;
  warningCount: number;
  errorCount: number;
}

interface ConversationSyncResultItem {
  session_count?: unknown;
  skipped_session_count?: unknown;
  turn_count?: unknown;
  warning_count?: unknown;
}

export function summarizeConversationSyncTask(
  task: ConversationSyncTaskSnapshot,
): ConversationSyncSummaryCounts | null {
  if (!isRecord(task.result)) {
    return null;
  }
  const results = Array.isArray(task.result.results) ? task.result.results : [];
  const errors = Array.isArray(task.result.errors) ? task.result.errors : [];
  if (results.length === 0 && errors.length === 0) {
    return null;
  }

  return results.reduce<ConversationSyncSummaryCounts>(
    (summary, rawResult) => {
      const result = isRecord(rawResult) ? (rawResult as ConversationSyncResultItem) : {};
      const sessionCount = numberValue(result.session_count);
      const skippedSessionCount = numberValue(result.skipped_session_count);
      return {
        sourceCount: summary.sourceCount + 1,
        changedSessionCount: summary.changedSessionCount + Math.max(0, sessionCount - skippedSessionCount),
        skippedSessionCount: summary.skippedSessionCount + skippedSessionCount,
        turnCount: summary.turnCount + numberValue(result.turn_count),
        warningCount: summary.warningCount + numberValue(result.warning_count),
        errorCount: summary.errorCount,
      };
    },
    {
      sourceCount: 0,
      changedSessionCount: 0,
      skippedSessionCount: 0,
      turnCount: 0,
      warningCount: 0,
      errorCount: errors.length,
    },
  );
}

export async function listConversationAdapters(): Promise<ConversationAdapter[]> {
  try {
    return await invoke<ConversationAdapter[]>("list_conversation_adapters");
  } catch (error) {
    if (isTauriRuntime()) {
      throw error;
    }

    return fallbackAdapters;
  }
}

export async function validateConversationAdapter(
  manifestPath: string,
): Promise<ConversationAdapterValidationResult> {
  try {
    return await invoke<ConversationAdapterValidationResult>("validate_conversation_adapter", {
      params: { manifest_path: manifestPath },
    });
  } catch (error) {
    if (isTauriRuntime()) {
      throw error;
    }

    return fallbackConversationAdapterValidation(manifestPath);
  }
}

export async function registerConversationAdapter(
  manifestPath: string,
  dryRun = false,
): Promise<ConversationAdapterRegisterResult> {
  try {
    return await invoke<ConversationAdapterRegisterResult>("register_conversation_adapter", {
      params: { dry_run: dryRun, manifest_path: manifestPath, yes: true },
    });
  } catch (error) {
    if (isTauriRuntime()) {
      throw error;
    }

    const validation = fallbackConversationAdapterValidation(manifestPath);
    return {
      dry_run: dryRun,
      adapter: conversationAdapterFromValidation(validation),
      validation,
    };
  }
}

export async function upsertConversationSource(
  source: ConversationSource,
  dryRun = false,
): Promise<ConversationSourceUpsertResult> {
  try {
    return await invoke<ConversationSourceUpsertResult>("upsert_conversation_source", {
      params: { dry_run: dryRun, source },
    });
  } catch (error) {
    if (isTauriRuntime()) {
      throw error;
    }

    return { dry_run: dryRun, source };
  }
}

export async function importConversationSource(
  params: ImportConversationSourceParams,
  onProgress?: (step: ImportConversationSourceProgress) => void,
  startSync: StartConversationSync = syncConversations,
): Promise<ImportConversationSourceResult> {
  onProgress?.("validating");
  const validation = await validateConversationAdapter(params.manifest_path);
  if (!validation.manifest.capabilities.includes("read_session")) {
    throw new Error("conversation adapter must declare read_session");
  }
  if (params.record_kind === "web" && !validation.manifest.capabilities.includes("web_records")) {
    throw new Error("web record imports require an adapter with web_records capability");
  }
  if (params.record_kind !== "web" && validation.manifest.capabilities.includes("web_records")) {
    throw new Error("web record adapters must be imported from the web records page");
  }
  if (!validation.manifest.input_kinds.includes(params.source_kind)) {
    throw new Error(`conversation adapter does not support source kind: ${params.source_kind}`);
  }

  onProgress?.("source");
  const registration = await registerConversationAdapter(validation.manifest_path, false);
  const nowIso = new Date().toISOString();
  const source: ConversationSource = {
    id:
      params.source_id?.trim() ||
      conversationSourceId(registration.adapter.id, params.source_location),
    adapter_id: registration.adapter.id,
    name: params.source_name.trim() || registration.adapter.name,
    kind: params.source_kind,
    location: params.source_location.trim(),
    config_json: normalizeOptionalJson(params.config_json),
    enabled: true,
    created_at: nowIso,
    updated_at: nowIso,
  };

  const upsert = await upsertConversationSource(source, false);
  onProgress?.("sync");
  const task = await startSync({
    dry_run: false,
    record_kind: params.record_kind ?? "session",
    source_id: upsert.source.id,
  });

  return {
    adapter: registration.adapter,
    source: upsert.source,
    task,
    validation,
  };
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function numberValue(value: unknown) {
  return typeof value === "number" && Number.isFinite(value) ? value : 0;
}

function normalizeOptionalJson(value: string | null | undefined) {
  const trimmed = value?.trim();
  if (!trimmed) {
    return null;
  }
  JSON.parse(trimmed);
  return trimmed;
}

function conversationSourceId(adapterId: string, location: string) {
  const locationSlug = location
    .trim()
    .toLowerCase()
    .replace(/^~\//, "home/")
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, 48);
  return [adapterId, locationSlug || "source"].join("-");
}

function fallbackConversationAdapterValidation(
  manifestPath: string,
): ConversationAdapterValidationResult {
  const webLike = /web|browser|qwen|chatgpt/i.test(manifestPath);
  return {
    valid: true,
    manifest_path: manifestPath,
    manifest_hash: "preview-manifest-hash",
    executable_path: `${manifestPath.replace(/\/[^/]*$/, "")}/adapter`,
    executable_hash: "preview-executable-hash",
    manifest: {
      schema_version: 1,
      id: webLike ? "preview-web-adapter" : "preview-conversation-adapter",
      name: webLike ? "Preview Web Adapter" : "Preview Conversation Adapter",
      version: "0.1.0",
      protocol_version: 1,
      command: ["adapter"],
      capabilities: webLike
        ? ["probe", "read_session", "web_records"]
        : ["probe", "read_session"],
      input_kinds: ["directory", "file", "sqlite"],
    },
    warnings: [],
  };
}

function conversationAdapterFromValidation(
  validation: ConversationAdapterValidationResult,
): ConversationAdapter {
  const nowIso = new Date().toISOString();
  return {
    id: validation.manifest.id,
    name: validation.manifest.name,
    kind: "external",
    version: validation.manifest.version,
    enabled: true,
    manifest_path: validation.manifest_path,
    executable_path: validation.executable_path,
    content_hash: validation.executable_hash,
    trusted_hash: validation.executable_hash ?? validation.manifest_hash,
    trust_state: "trusted",
    protocol_version: validation.manifest.protocol_version,
    capabilities: validation.manifest.capabilities,
    input_kinds: validation.manifest.input_kinds,
    created_at: nowIso,
    updated_at: nowIso,
  };
}

export async function listConversationSources(): Promise<ConversationSource[]> {
  try {
    return await invoke<ConversationSource[]>("list_conversation_sources");
  } catch (error) {
    if (isTauriRuntime()) {
      throw error;
    }

    return fallbackSources;
  }
}

export async function syncConversations(
  params: {
    source_id?: string | null;
    adapter_id?: string | null;
    dry_run?: boolean;
    record_kind?: ConversationRecordKind | null;
  },
): Promise<ConversationSyncTaskSnapshot> {
  try {
    return await invoke<ConversationSyncTaskSnapshot>("sync_conversations", { params });
  } catch (error) {
    if (isTauriRuntime()) {
      throw error;
    }

    const recordKind = params.record_kind ?? "session";
    return {
      id: "preview-conversation-sync",
      status: "completed",
      source_id: params.source_id ?? null,
      adapter_id: params.adapter_id ?? null,
      record_kind: recordKind,
      dry_run: Boolean(params.dry_run),
      started_at: new Date().toISOString(),
      finished_at: new Date().toISOString(),
      result: {
        dry_run: Boolean(params.dry_run),
        errors: [],
        results: [
          {
            source_id: "codex-live",
            adapter_id: "codex",
            dry_run: Boolean(params.dry_run),
            record_kind: recordKind,
            session_count: fallbackSessions.length,
            skipped_session_count: 0,
            turn_count: fallbackSessions.reduce((total, session) => total + session.turn_count, 0),
            warning_count: 0,
            warnings: [],
          },
        ],
      },
      error: null,
    };
  }
}

export async function getConversationSyncTask(): Promise<ConversationSyncTaskSnapshot | null> {
  try {
    return await invoke<ConversationSyncTaskSnapshot | null>("get_conversation_sync_task");
  } catch (error) {
    if (isTauriRuntime()) {
      throw error;
    }
    return null;
  }
}

export async function listConversationSessions(params: ConversationSessionListParams): Promise<ConversationSessionListItem[]> {
  try {
    return await invoke<ConversationSessionListItem[]>("list_conversation_sessions", { params });
  } catch (error) {
    if (isTauriRuntime()) {
      throw error;
    }

    return fallbackSessions.filter((session) => {
      if (params.adapter_id && session.adapter_id !== params.adapter_id) return false;
      if (params.source_id && session.source_id !== params.source_id) return false;
      if (params.query && !`${session.title} ${session.project_path ?? ""}`.toLowerCase().includes(params.query.toLowerCase())) return false;
      return true;
    });
  }
}

export async function getConversationSession(sessionId: string): Promise<ConversationSessionDetail> {
  try {
    return await invoke<ConversationSessionDetail>("get_conversation_session", { params: { session_id: sessionId } });
  } catch (error) {
    if (isTauriRuntime()) {
      throw error;
    }

    return fallbackSessionDetail;
  }
}

export async function listWebRecordSessions(params: ConversationSessionListParams): Promise<ConversationSessionListItem[]> {
  try {
    return await invoke<ConversationSessionListItem[]>("list_web_record_sessions", { params });
  } catch (error) {
    if (isTauriRuntime()) {
      throw error;
    }

    return fallbackWebSessions.filter((session) => {
      if (params.adapter_id && session.adapter_id !== params.adapter_id) return false;
      if (params.source_id && session.source_id !== params.source_id) return false;
      if (params.query && !`${session.title} ${session.project_path ?? ""}`.toLowerCase().includes(params.query.toLowerCase())) return false;
      return true;
    });
  }
}

export async function getWebRecordSession(sessionId: string): Promise<ConversationSessionDetail> {
  try {
    return await invoke<ConversationSessionDetail>("get_web_record_session", { params: { session_id: sessionId } });
  } catch (error) {
    if (isTauriRuntime()) {
      throw error;
    }

    return fallbackWebSessionDetail;
  }
}

export async function searchConversationRecords(params: ConversationSearchParams): Promise<ConversationSearchResult> {
  const trimmedQuery = params.query.trim();
  if (!trimmedQuery) {
    const recordKind = params.record_kind ?? "session";
    const limit = params.limit ?? 50;
    const offset = params.offset ?? 0;
    return {
      query: "",
      record_kind: recordKind,
      scope: conversationSearchScope({
        ...params,
        query: "",
        record_kind: recordKind,
        content_types: params.content_types ?? [],
        limit,
        offset,
        timeline: params.timeline ?? false,
      }),
      total_count: 0,
      hits: [],
    };
  }

  const payload = {
    ...params,
    query: trimmedQuery,
    record_kind: params.record_kind ?? "session",
    content_types: params.content_types ?? [],
    since: params.since ?? null,
    until: params.until ?? null,
    timeline: params.timeline ?? false,
    limit: params.limit ?? 50,
    offset: params.offset ?? 0,
  };

  try {
    return await invoke<ConversationSearchResult>("search_conversation_records", { params: payload });
  } catch (error) {
    if (isTauriRuntime()) {
      throw error;
    }

    return fallbackConversationSearch(payload);
  }
}

export async function listConversationQuestions(params: ConversationQuestionListParams): Promise<ConversationQuestionDetail[]> {
  try {
    return await invoke<ConversationQuestionDetail[]>("list_conversation_questions", { params });
  } catch (error) {
    if (isTauriRuntime()) {
      throw error;
    }

    return fallbackSessionDetail.questions;
  }
}

export async function getConversationQuestion(questionId: string): Promise<ConversationQuestionDetail> {
  try {
    return await invoke<ConversationQuestionDetail>("get_conversation_question", { params: { question_id: questionId } });
  } catch (error) {
    if (isTauriRuntime()) {
      throw error;
    }

    return fallbackSessionDetail.questions.find((question) => question.question.id === questionId) ?? fallbackSessionDetail.questions[0];
  }
}

export async function mergeConversationQuestions(questionIds: string[], dryRun = false): Promise<ConversationMutationResult> {
  try {
    return await invoke<ConversationMutationResult>("merge_conversation_questions", {
      params: { question_ids: questionIds, dry_run: dryRun },
    });
  } catch (error) {
    if (isTauriRuntime()) {
      throw error;
    }

    return {
      dry_run: dryRun,
      session_id: fallbackSessionDetail.session.id,
      affected_question_ids: questionIds,
      questions: fallbackSessionDetail.questions.filter((question) => questionIds.includes(question.question.id)),
    };
  }
}

export async function splitConversationQuestion(questionId: string, beforeTurnId: string, dryRun = false): Promise<ConversationMutationResult> {
  try {
    return await invoke<ConversationMutationResult>("split_conversation_question", {
      params: { question_id: questionId, before_turn_id: beforeTurnId, dry_run: dryRun },
    });
  } catch (error) {
    if (isTauriRuntime()) {
      throw error;
    }

    return {
      dry_run: dryRun,
      session_id: fallbackSessionDetail.session.id,
      affected_question_ids: [questionId],
      questions: fallbackSessionDetail.questions.filter((question) => question.question.id === questionId || question.turns.some((turn) => turn.id === beforeTurnId)),
    };
  }
}

export async function exportConversationSession(
  sessionId: string,
  outputRoot: string,
  dryRun = false,
  questionIds: string[] = [],
  contentFilter?: ConversationExportContentFilter,
) {
  const resolvedContentFilter = contentFilter ?? {
    answer: true,
    code: true,
    command: true,
    result: true,
    tool: true,
  };
  try {
    return await invoke("export_conversation_session", {
      params: {
        session_id: sessionId,
        output_root: outputRoot,
        question_ids: questionIds,
        content_filter: resolvedContentFilter,
        dry_run: dryRun,
      },
    });
  } catch (error) {
    if (isTauriRuntime()) {
      throw error;
    }

    return {
      dry_run: dryRun,
      session_id: sessionId,
      question_ids: questionIds,
      output_path: `${outputRoot}/codex/preview-project/preview-conversation-session-preview.md`,
    };
  }
}

export async function exportWebRecordSession(
  sessionId: string,
  outputRoot: string,
  dryRun = false,
  questionIds: string[] = [],
  contentFilter?: ConversationExportContentFilter,
) {
  const resolvedContentFilter = contentFilter ?? {
    answer: true,
    code: true,
    command: true,
    result: true,
    tool: true,
  };
  try {
    return await invoke("export_web_record_session", {
      params: {
        session_id: sessionId,
        output_root: outputRoot,
        question_ids: questionIds,
        content_filter: resolvedContentFilter,
        dry_run: dryRun,
      },
    });
  } catch (error) {
    if (isTauriRuntime()) {
      throw error;
    }

    return {
      dry_run: dryRun,
      session_id: sessionId,
      question_ids: questionIds,
      output_path: `${outputRoot}/qwen-web/web/preview-web-record.md`,
    };
  }
}

function fallbackConversationSearch(params: Required<Pick<ConversationSearchParams, "query" | "record_kind" | "content_types" | "limit" | "offset" | "timeline">> & ConversationSearchParams): ConversationSearchResult {
  const detail = params.record_kind === "web" ? fallbackWebSessionDetail : fallbackSessionDetail;
  const session = params.record_kind === "web" ? fallbackWebSessions[0] : fallbackSessions[0];
  const needle = params.query.trim().toLowerCase();
  if (params.project_path && session.project_path !== params.project_path) {
    return {
      query: params.query,
      record_kind: params.record_kind,
      scope: conversationSearchScope(params),
      total_count: 0,
      hits: [],
    };
  }
  if (!conversationSessionWithinSearchTime(session, params.since, params.until)) {
    return {
      query: params.query,
      record_kind: params.record_kind,
      scope: conversationSearchScope(params),
      total_count: 0,
      hits: [],
    };
  }
  const allowedTypes = new Set(params.content_types);
  const hits: ConversationSearchHit[] = [];

  for (const questionDetail of detail.questions) {
    const questionTitle = questionDetail.question.title || firstLine(questionDetail.question.question_text);
    for (const turn of questionDetail.turns) {
      pushFallbackHit(hits, {
        allowedTypes,
        blockId: `${turn.id}-question`,
        cardType: "question",
        needle,
        partId: null,
        questionDetail,
        questionTitle,
        session,
        text: turn.user_text,
        turnId: turn.id,
      });

      for (const part of questionDetail.parts.filter((candidate) => candidate.turn_id === turn.id)) {
        for (const entry of fallbackEntriesForPart(part)) {
          pushFallbackHit(hits, {
            allowedTypes,
            blockId: entry.blockId,
            cardType: entry.cardType,
            needle,
            partId: part.id,
            questionDetail,
            questionTitle,
            session,
            text: entry.text,
            turnId: turn.id,
          });
        }
      }
    }
  }

  return {
    query: params.query,
    record_kind: params.record_kind,
    scope: conversationSearchScope(params),
    total_count: hits.length,
    hits: hits.slice(params.offset, params.offset + params.limit),
  };
}

function conversationSearchScope(params: Required<Pick<ConversationSearchParams, "query" | "record_kind" | "content_types" | "limit" | "offset" | "timeline">> & ConversationSearchParams): ConversationSearchScope {
  return {
    record_kind: params.record_kind,
    adapter_id: params.adapter_id ?? null,
    source_id: params.source_id ?? null,
    project_path: params.project_path ?? null,
    query: params.query,
    content_types: params.content_types,
    since: params.since ?? null,
    until: params.until ?? null,
    timeline: params.timeline,
    limit: params.limit,
    offset: params.offset,
  };
}

function conversationSessionWithinSearchTime(session: ConversationSessionListItem, since?: string | null, until?: string | null) {
  if (!since && !until) return true;
  const sessionTime = Date.parse(session.started_at ?? session.updated_at ?? session.imported_at);
  if (!Number.isFinite(sessionTime)) return false;
  const sinceTime = since ? Date.parse(searchDateBound(since, "start")) : Number.NEGATIVE_INFINITY;
  const untilTime = until ? Date.parse(searchDateBound(until, "end")) : Number.POSITIVE_INFINITY;
  return sessionTime >= sinceTime && sessionTime <= untilTime;
}

function searchDateBound(value: string, bound: "start" | "end") {
  return /^\d{4}-\d{2}-\d{2}$/.test(value)
    ? `${value}T${bound === "start" ? "00:00:00.000Z" : "23:59:59.999Z"}`
    : value;
}

function pushFallbackHit(
  hits: ConversationSearchHit[],
  params: {
    allowedTypes: Set<ConversationSearchCardType>;
    blockId: string;
    cardType: ConversationSearchCardType;
    needle: string;
    partId: string | null;
    questionDetail: ConversationQuestionDetail;
    questionTitle: string;
    session: ConversationSessionListItem;
    text?: string | null;
    turnId: string;
  },
) {
  const text = params.text?.trim();
  if (!text) return;
  if (params.allowedTypes.size > 0 && !params.allowedTypes.has(params.cardType)) return;
  if (!text.toLowerCase().includes(params.needle)) return;

  hits.push({
    block_id: params.blockId,
    card_type: params.cardType,
    part_id: params.partId,
    question_id: params.questionDetail.question.id,
    question_index: params.questionDetail.question.question_index,
    question_title: params.questionTitle,
    score: Math.max(1, text.toLowerCase().split(params.needle).length - 1) * 100,
    session: params.session,
    snippet: fallbackSnippet(text, params.needle),
    turn_id: params.turnId,
  });
}

function fallbackEntriesForPart(part: ConversationQuestionDetail["parts"][number]) {
  const declaredCard = declaredContentCard(part.metadata_json);
  if (!declaredCard) return [];

  const cardType = declaredCard.type;
  const primaryText = declaredCard.text
    ?? (cardType === "command"
      ? part.command ?? part.text
      : part.text ?? part.command);
  return fallbackEntry(part.id, cardType, primaryText, declaredCard.suffix ?? cardType);
}

function fallbackEntry(
  partId: string,
  cardType: ConversationSearchCardType,
  text?: string | null,
  suffix: string = cardType,
) {
  const trimmedText = text?.trim();
  return trimmedText ? [{ blockId: `${partId}-${suffix}`, cardType, text: trimmedText }] : [];
}

interface DeclaredContentCard {
  suffix?: string;
  text?: string;
  type: ConversationSearchCardType;
}

function declaredContentCard(metadataJson?: string | null): DeclaredContentCard | null {
  const metadata = parseMetadata(metadataJson);
  if (!metadata) return null;
  const card = metadata.content_card ?? metadata.contentCard;
  if (!isRecord(card)) return null;
  const type = card.type;
  if (
    type === "answer"
    || type === "tool"
    || type === "command"
    || type === "code"
    || type === "result"
  ) {
    return {
      suffix: stringValue(card.suffix),
      text: stringValue(card.text),
      type,
    };
  }
  return null;
}

function stringValue(value: unknown) {
  return typeof value === "string" && value.trim() ? value : undefined;
}

function parseMetadata(metadataJson?: string | null): Record<string, unknown> | null {
  if (!metadataJson?.trim()) return null;
  try {
    const parsed = JSON.parse(metadataJson);
    return isRecord(parsed) ? parsed : null;
  } catch {
    return null;
  }
}

function fallbackSnippet(text: string, needle: string) {
  const index = text.toLowerCase().indexOf(needle);
  const start = Math.max(0, index - 64);
  const end = Math.min(text.length, index + needle.length + 96);
  return `${start > 0 ? "..." : ""}${text.slice(start, end)}${end < text.length ? "..." : ""}`
    .split(/\s+/)
    .join(" ");
}

function firstLine(value: string) {
  return value.split(/\r?\n/).find((line) => line.trim())?.trim() ?? "Untitled question";
}

function isTauriRuntime() {
  return typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
}

const now = new Date().toISOString();

const fallbackAdapters: ConversationAdapter[] = [
  {
    id: "codex",
    name: "Codex",
    kind: "external",
    version: "1.0.0",
    enabled: true,
    manifest_path: "~/.assetiweave/conversation-adapters/codex/conversation-adapter.json",
    executable_path: "~/.assetiweave/conversation-adapters/codex/adapter.mjs",
    trust_state: "built_in",
    capabilities: ["probe", "list_sessions", "read_session"],
    input_kinds: ["live", "file"],
    created_at: now,
    updated_at: now,
  },
  {
    id: "claude-code",
    name: "Claude Code",
    kind: "external",
    version: "1.0.0",
    enabled: true,
    manifest_path: "~/.assetiweave/conversation-adapters/claude-code/conversation-adapter.json",
    executable_path: "~/.assetiweave/conversation-adapters/claude-code/adapter.mjs",
    trust_state: "built_in",
    capabilities: ["probe", "list_sessions", "read_session"],
    input_kinds: ["live", "directory", "file"],
    created_at: now,
    updated_at: now,
  },
  {
    id: "opencode",
    name: "OpenCode",
    kind: "external",
    version: "1.0.0",
    enabled: true,
    manifest_path: "~/.assetiweave/conversation-adapters/opencode/conversation-adapter.json",
    executable_path: "~/.assetiweave/conversation-adapters/opencode/adapter.mjs",
    trust_state: "built_in",
    capabilities: ["probe", "list_sessions", "read_session"],
    input_kinds: ["live", "sqlite"],
    created_at: now,
    updated_at: now,
  },
  {
    id: "qwen-web",
    name: "Qwen Web",
    kind: "external",
    version: "0.1.0",
    enabled: true,
    trust_state: "trusted",
    capabilities: ["probe", "read_session", "web_records"],
    input_kinds: ["directory"],
    created_at: now,
    updated_at: now,
  },
];

const fallbackSources: ConversationSource[] = [
  {
    id: "codex-live",
    adapter_id: "codex",
    name: "Codex local sessions",
    kind: "live",
    location: "~/.codex",
    enabled: true,
    created_at: now,
    updated_at: now,
  },
];

const fallbackSessions: ConversationSessionListItem[] = [
  {
    id: "preview-session",
    source_id: "codex-live",
    adapter_id: "codex",
    external_id: "preview",
    title: "Preview conversation session",
    project_path: "/preview/project",
    missing: false,
    created_at: now,
    imported_at: now,
    question_count: 2,
    turn_count: 3,
  },
];

const fallbackSessionDetail: ConversationSessionDetail = {
  session: fallbackSessions[0],
  questions: [
    {
      question: {
        id: "preview-question-1",
        session_id: "preview-session",
        question_index: 0,
        title: "How does conversation sync work?",
        question_text: "How does conversation sync work?\n\n继续",
        answer_text: "AssetIWeave imports source sessions into normalized turns, then groups adjacent turns into question records.",
        code_text: "",
        command_text: "assetiweave-cli conversation sync --source codex-live",
        grouping_origin: "auto_merged",
        created_at: now,
        updated_at: now,
      },
      turns: [
        {
          id: "preview-turn-1",
          session_id: "preview-session",
          external_id: "turn-1",
          turn_index: 0,
          user_text: "How does conversation sync work?",
          fingerprint: "preview",
          missing: false,
          imported_at: now,
        },
        {
          id: "preview-turn-2",
          session_id: "preview-session",
          external_id: "turn-2",
          turn_index: 1,
          user_text: "继续",
          fingerprint: "preview",
          missing: false,
          imported_at: now,
        },
      ],
      parts: [
        {
          id: "preview-part-1",
          turn_id: "preview-turn-1",
          part_index: 0,
          role: "assistant",
          kind: "text",
          text: "AssetIWeave imports source sessions into normalized turns, then groups adjacent turns into question records.",
          metadata_json: JSON.stringify({
            content_card: { type: "answer", format: "markdown" },
          }),
        },
        {
          id: "preview-part-2",
          turn_id: "preview-turn-2",
          part_index: 0,
          role: "tool",
          kind: "command",
          command: "assetiweave-cli conversation sync --source codex-live",
          metadata_json: JSON.stringify({
            content_card: { type: "command" },
          }),
        },
      ],
    },
    {
      question: {
        id: "preview-question-2",
        session_id: "preview-session",
        question_index: 1,
        title: "Export this session",
        question_text: "Export this session",
        answer_text: "Use session export to write one Markdown file per session.",
        code_text: "",
        command_text: "",
        grouping_origin: "imported",
        created_at: now,
        updated_at: now,
      },
      turns: [
        {
          id: "preview-turn-3",
          session_id: "preview-session",
          external_id: "turn-3",
          turn_index: 2,
          user_text: "Export this session",
          fingerprint: "preview",
          missing: false,
          imported_at: now,
        },
      ],
      parts: [
        {
          id: "preview-part-3",
          turn_id: "preview-turn-3",
          part_index: 0,
          role: "assistant",
          kind: "text",
          text: "Use session export to write one Markdown file per session.",
          metadata_json: JSON.stringify({
            content_card: { type: "answer", format: "markdown" },
          }),
        },
      ],
    },
  ],
};

const fallbackWebSessions: ConversationSessionListItem[] = [
  {
    ...fallbackSessions[0],
    id: "preview-web-record",
    source_id: "qwen-web-export",
    adapter_id: "qwen-web",
    external_id: "qwen-preview",
    title: "Qwen web conversation",
    project_path: null,
  },
];

const fallbackWebSessionDetail: ConversationSessionDetail = {
  session: fallbackWebSessions[0],
  questions: fallbackSessionDetail.questions.map((detail, questionIndex) => ({
    ...detail,
    question: {
      ...detail.question,
      id: `preview-web-question-${questionIndex + 1}`,
      session_id: fallbackWebSessions[0].id,
    },
    turns: detail.turns.map((turn, turnIndex) => ({
      ...turn,
      id: `preview-web-turn-${questionIndex + 1}-${turnIndex + 1}`,
      session_id: fallbackWebSessions[0].id,
    })),
    parts: detail.parts.map((part, partIndex) => ({
      ...part,
      id: `preview-web-part-${questionIndex + 1}-${partIndex + 1}`,
      turn_id: `preview-web-turn-${questionIndex + 1}-${Math.min(partIndex + 1, detail.turns.length)}`,
    })),
  })),
};
