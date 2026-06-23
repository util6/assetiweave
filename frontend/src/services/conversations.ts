import { invoke } from "@tauri-apps/api/core";
import type {
  ConversationAdapter,
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

export interface ConversationEntryAddParams {
  plugin_path?: string | null;
  plugin_id?: string | null;
  manifest_path?: string | null;
  source_id?: string | null;
  source_name: string;
  source_kind: "live" | "file" | "directory" | "sqlite" | "custom";
  location: string;
  config_json?: string | null;
  record_kind: ConversationRecordKind;
  dry_run?: boolean;
  yes?: boolean;
  sync_after_add?: boolean;
}

export interface ConversationEntryAddResult {
  dry_run: boolean;
  record_kind: ConversationRecordKind;
  plugin_directory?: string | null;
  manifest_path: string;
  adapter: ConversationAdapter;
  source: ConversationSource;
  sync_result?: unknown | null;
}

export type ConversationSyncTaskStatus = "running" | "completed" | "failed";

export interface ConversationSyncTaskSnapshot {
  id: string;
  status: ConversationSyncTaskStatus;
  source_id: string | null;
  adapter_id: string | null;
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

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function numberValue(value: unknown) {
  return typeof value === "number" && Number.isFinite(value) ? value : 0;
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

export async function addConversationEntry(
  params: ConversationEntryAddParams,
): Promise<ConversationEntryAddResult> {
  try {
    return await invoke<ConversationEntryAddResult>("add_conversation_entry", { params });
  } catch (error) {
    if (isTauriRuntime()) {
      throw error;
    }

    const now = new Date().toISOString();
    const manifestPath = params.manifest_path
      ?? (params.plugin_path ? `${params.plugin_path.replace(/\/$/, "")}/conversation-adapter.json` : null)
      ?? "/tmp/plugin/conversation-adapter.json";
    return {
      dry_run: Boolean(params.dry_run),
      record_kind: params.record_kind,
      plugin_directory: params.plugin_path ?? null,
      manifest_path: manifestPath,
      adapter: {
        id: "preview-plugin",
        name: "Preview Plugin",
        kind: "external",
        version: "0.1.0",
        enabled: true,
        manifest_path: manifestPath,
        executable_path: null,
        content_hash: null,
        trusted_hash: null,
        trust_state: "trusted",
        protocol_version: 1,
        capabilities: params.record_kind === "web"
          ? ["read_session", "web_records"]
          : ["read_session"],
        input_kinds: [params.source_kind],
        created_at: now,
        updated_at: now,
      },
      source: {
        id: params.source_id || "preview-plugin-source",
        adapter_id: "preview-plugin",
        name: params.source_name,
        kind: params.source_kind,
        location: params.location,
        config_json: params.config_json ?? null,
        enabled: true,
        last_synced_at: null,
        last_sync_status: null,
        created_at: now,
        updated_at: now,
      },
      sync_result: null,
    };
  }
}

export async function syncConversations(
  params: {
    source_id?: string | null;
    adapter_id?: string | null;
    record_kind?: ConversationRecordKind | null;
    dry_run?: boolean;
  },
): Promise<ConversationSyncTaskSnapshot> {
  try {
    return await invoke<ConversationSyncTaskSnapshot>("sync_conversations", { params });
  } catch (error) {
    if (isTauriRuntime()) {
      throw error;
    }

    return {
      id: "preview-conversation-sync",
      status: "completed",
      source_id: params.source_id ?? null,
      adapter_id: params.adapter_id ?? null,
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

    return fallbackWebSessionDetails.get(sessionId) ?? fallbackWebSessionDetail;
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
      output_path: `${outputRoot}/${fallbackWebSessionSiteId(sessionId)}/web/preview-web-record.md`,
    };
  }
}

function fallbackConversationSearch(params: Required<Pick<ConversationSearchParams, "query" | "record_kind" | "content_types" | "limit" | "offset" | "timeline">> & ConversationSearchParams): ConversationSearchResult {
  const session = params.record_kind === "web" ? fallbackWebSearchSession(params) : fallbackSessions[0];
  const detail = params.record_kind === "web"
    ? fallbackWebSessionDetails.get(session.id) ?? fallbackWebSessionDetail
    : fallbackSessionDetail;
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

function fallbackWebSearchSession(params: ConversationSearchParams) {
  return (
    fallbackWebSessions.find((session) => params.adapter_id && session.adapter_id === params.adapter_id) ??
    fallbackWebSessions.find((session) => params.source_id && session.source_id === params.source_id) ??
    fallbackWebSessions[0]
  );
}

function fallbackWebSessionSiteId(sessionId: string) {
  return fallbackWebSessions.find((session) => session.id === sessionId)?.adapter_id ?? "qwen-web";
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
  if (part.kind === "code_block") {
    return fallbackEntry(part.id, "code", part.text);
  }
  if (part.kind === "command") {
    const command = part.command?.trim() || part.text?.trim();
    const output = commandResultText(part);
    return [
      ...fallbackEntry(part.id, "command", command, "command"),
      ...fallbackEntry(part.id, "result", output, "result"),
    ];
  }
  if (part.kind === "text") {
    const cardType = part.role === "tool" ? "result" : "answer";
    return fallbackEntry(part.id, cardType, part.text);
  }

  const cardType = isResultPart(part) ? "result" : "tool";
  return fallbackEntry(part.id, cardType, part.text ?? part.metadata_json);
}

function fallbackEntry(
  partId: string,
  cardType: ConversationSearchCardType,
  text?: string | null,
  suffix = cardType,
) {
  const trimmedText = text?.trim();
  return trimmedText ? [{ blockId: `${partId}-${suffix}`, cardType, text: trimmedText }] : [];
}

function commandResultText(part: ConversationQuestionDetail["parts"][number]) {
  const text = part.text?.trim();
  if (text && text !== part.command?.trim()) return text;
  if (part.status) return part.status;
  if (part.exit_code != null) return `Exit code ${part.exit_code}`;
  return null;
}

function isResultPart(part: ConversationQuestionDetail["parts"][number]) {
  if (part.role === "tool" && part.kind === "text") return true;
  if (part.status || part.exit_code != null) return true;
  const metadata = part.metadata_json?.toLowerCase() ?? "";
  return [
    "tool_result",
    "tool-result",
    "tool_output",
    "tooloutput",
    "function_call_output",
    "\"output\"",
    "\"result\"",
  ].some((marker) => metadata.includes(marker));
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
    kind: "codex",
    version: "1",
    enabled: true,
    trust_state: "built_in",
    capabilities: ["probe", "list_sessions", "read_session"],
    input_kinds: ["live", "file"],
    created_at: now,
    updated_at: now,
  },
  {
    id: "opencode",
    name: "OpenCode",
    kind: "opencode",
    version: "1",
    enabled: true,
    trust_state: "built_in",
    capabilities: ["probe", "list_sessions", "read_session"],
    input_kinds: ["live", "sqlite"],
    created_at: now,
    updated_at: now,
  },
  {
    id: "qwen-web",
    name: "Qwen Web",
    ...fallbackWebAdapterFields("0.1.1"),
  },
  {
    id: "chatgpt-web",
    name: "ChatGPT Web",
    ...fallbackWebAdapterFields("0.1.0"),
  },
];

function fallbackWebAdapterFields(version: string): Omit<ConversationAdapter, "id" | "name"> {
  return {
    kind: "external",
    version,
    enabled: true,
    trust_state: "trusted",
    capabilities: ["probe", "read_session", "web_records"],
    input_kinds: ["directory"],
    created_at: now,
    updated_at: now,
  };
}

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
        },
        {
          id: "preview-part-2",
          turn_id: "preview-turn-2",
          part_index: 0,
          role: "tool",
          kind: "command",
          command: "assetiweave-cli conversation sync --source codex-live",
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
  {
    ...fallbackSessions[0],
    id: "preview-chatgpt-web-record",
    source_id: "chatgpt-web-export",
    adapter_id: "chatgpt-web",
    external_id: "chatgpt-preview",
    title: "ChatGPT web conversation",
    project_path: null,
  },
];

const fallbackWebSessionDetails = new Map(
  fallbackWebSessions.map((session) => [session.id, buildFallbackWebSessionDetail(session)]),
);

const fallbackWebSessionDetail = fallbackWebSessionDetails.get(fallbackWebSessions[0].id) ?? buildFallbackWebSessionDetail(fallbackWebSessions[0]);

function buildFallbackWebSessionDetail(session: ConversationSessionListItem): ConversationSessionDetail {
  return {
    session,
    questions: fallbackSessionDetail.questions.map((detail, questionIndex) => ({
      ...detail,
      question: {
        ...detail.question,
        id: `${session.id}-question-${questionIndex + 1}`,
        session_id: session.id,
      },
      turns: detail.turns.map((turn, turnIndex) => ({
        ...turn,
        id: `${session.id}-turn-${questionIndex + 1}-${turnIndex + 1}`,
        session_id: session.id,
      })),
      parts: detail.parts.map((part, partIndex) => ({
        ...part,
        id: `${session.id}-part-${questionIndex + 1}-${partIndex + 1}`,
        turn_id: `${session.id}-turn-${questionIndex + 1}-${Math.min(partIndex + 1, detail.turns.length)}`,
      })),
    })),
  };
}
