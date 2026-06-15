import { invoke } from "@tauri-apps/api/core";
import type {
  ConversationAdapter,
  ConversationMutationResult,
  ConversationQuestionDetail,
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

export interface ConversationExportContentFilter {
  answer: boolean;
  tool: boolean;
  command: boolean;
  code: boolean;
  result: boolean;
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

export async function syncConversations(params: { source_id?: string | null; adapter_id?: string | null; dry_run?: boolean }) {
  try {
    return await invoke("sync_conversations", { params });
  } catch (error) {
    if (isTauriRuntime()) {
      throw error;
    }

    return {
      dry_run: Boolean(params.dry_run),
      errors: [],
      results: [
        {
          source_id: "codex-live",
          adapter_id: "codex",
          dry_run: Boolean(params.dry_run),
          session_count: fallbackSessions.length,
          turn_count: fallbackSessions.reduce((total, session) => total + session.turn_count, 0),
          warning_count: 0,
          warnings: [],
        },
      ],
    };
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
    id: "claude-code",
    name: "Claude Code",
    kind: "claude_code",
    version: "1",
    enabled: true,
    trust_state: "built_in",
    capabilities: ["probe", "list_sessions", "read_session"],
    input_kinds: ["live", "directory", "file"],
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
