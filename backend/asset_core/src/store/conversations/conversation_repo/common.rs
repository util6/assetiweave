use super::*;

pub const CONVERSATION_IMPORT_BATCH_SIZE: usize = 8;

#[derive(Debug, Clone)]
pub struct RecentConversationSessionRecord {
    pub session: ConversationSessionListItem,
    pub last_activity_at: String,
    pub cwd: Option<String>,
    pub source_agent: String,
    pub recent_events: Vec<crate::backend::domain::RecentMemoryEvent>,
}

pub(super) fn builtin_sources(now: &str) -> Vec<ConversationSource> {
    vec![
        ConversationSource {
            id: "codex-live".to_string(),
            adapter_id: "codex".to_string(),
            name: "Codex local sessions".to_string(),
            kind: ConversationSourceKind::Live,
            location: "~/.codex".to_string(),
            config_json: None,
            enabled: true,
            last_synced_at: None,
            last_sync_status: None,
            created_at: now.to_string(),
            updated_at: now.to_string(),
        },
        ConversationSource {
            id: "claude-code-live".to_string(),
            adapter_id: "claude-code".to_string(),
            name: "Claude Code local sessions".to_string(),
            kind: ConversationSourceKind::Live,
            location: "~/.claude/projects".to_string(),
            config_json: None,
            enabled: true,
            last_synced_at: None,
            last_sync_status: None,
            created_at: now.to_string(),
            updated_at: now.to_string(),
        },
        ConversationSource {
            id: "opencode-live".to_string(),
            adapter_id: "opencode".to_string(),
            name: "OpenCode local sessions".to_string(),
            kind: ConversationSourceKind::Live,
            location: "~/.local/share/opencode/opencode.db".to_string(),
            config_json: None,
            enabled: true,
            last_synced_at: None,
            last_sync_status: None,
            created_at: now.to_string(),
            updated_at: now.to_string(),
        },
        ConversationSource {
            id: "codebuddy-live".to_string(),
            adapter_id: "codebuddy".to_string(),
            name: "CodeBuddy local sessions".to_string(),
            kind: ConversationSourceKind::Live,
            location: "~/.codebuddy".to_string(),
            config_json: None,
            enabled: true,
            last_synced_at: None,
            last_sync_status: None,
            created_at: now.to_string(),
            updated_at: now.to_string(),
        },
    ]
}

pub use super::row_mappers::*;

// 内部运行时目录白名单：只有真正落在应用运行时工作区里的会话才丢弃。
// 不能用 `.assetiweave` 前缀判定——用户的正常会话读 memory / 写 library 时
// projectPath 也会指向 `~/.assetiweave/...`，用前缀会把用户会话整条吞掉。
const INTERNAL_EXECUTION_DIR_SEGMENTS: [&str; 6] = [
    "/agent-executions/",
    "/agent-runs/",
    "/agent-runtimes/",
    "/smoke_workspace/",
    "/.system_generated/",
    "/test-scratch/",
];

pub(super) fn is_internal_execution_workspace_path(project_path: Option<&str>) -> bool {
    let Some(path) = project_path else {
        return false;
    };
    let normalized = path.replace('\\', "/").to_lowercase();
    // 补一个尾斜杠，兼容路径正好停在运行时目录名上的情况。
    let padded = format!("{normalized}/");
    INTERNAL_EXECUTION_DIR_SEGMENTS
        .iter()
        .any(|segment| padded.contains(segment))
}

/// 把单个路径折叠为便携锚点（`/Users/me/x` → `~/x`）。
///
/// `project_path` / `source_locator` / `last_cwd` 都由外部 adapter 的原生输出带进来，
/// 天生是主机绝对路径。直接落库有三个后果：
/// 1. 同一项目在不同批次、不同机器的字符串不同，前端按 `project_path` 分组的键会裂成多组；
/// 2. `is_session_unchanged` 的字符串比较会在「锚点 vs 绝对路径」之间反复判定为已变更而回写，
///    折叠永远稳不住；
/// 3. 数据库无法跨机器迁移。
///
/// 契约见 ADR-0020：写入一律折叠，展示层由前端 `abbreviateHomePath` 还原，
/// 需要真实文件路径时再走 `expand_path`。无法折叠的路径（`/tmp`、外接卷）原样保留。
pub(super) fn fold_path_for_storage(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    crate::backend::store::path_codec::normalize_path_for_storage(trimmed)
        .unwrap_or_else(|_| trimmed.to_string())
}

/// `Option` 形态的 `fold_path_for_storage`：空值与全空白折叠为 `None`。
pub(super) fn fold_optional_path(raw: Option<&str>) -> Option<String> {
    raw.map(fold_path_for_storage)
        .filter(|value| !value.is_empty())
}

pub(super) fn is_droppable_internal_session(
    normalized: &NormalizedConversationSession,
    session: &ConversationSession,
) -> bool {
    is_internal_execution_workspace_path(session.project_path.as_deref())
        || is_internal_execution_workspace_path(normalized.project_path.as_deref())
}

pub(super) fn conversation_session_from_normalized(
    source: &ConversationSource,
    normalized: &NormalizedConversationSession,
    now: &str,
) -> ConversationSession {
    let is_agent_workspace =
        is_internal_execution_workspace_path(normalized.project_path.as_deref());
    let execution_origin = normalized.execution_origin.clone().unwrap_or_else(|| {
        if is_agent_workspace {
            "internal_agent".to_string()
        } else {
            "user".to_string()
        }
    });
    let execution_purpose = normalized.execution_purpose.clone();
    let user_visible = normalized
        .user_visible
        .unwrap_or_else(|| !execution_origin.starts_with("internal_"));

    ConversationSession {
        id: stable_id(
            "conversation-session",
            &[&source.id, &normalized.external_id],
        ),
        source_id: source.id.clone(),
        adapter_id: source.adapter_id.clone(),
        external_id: normalized.external_id.clone(),
        title: normalized
            .title
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .unwrap_or("Untitled session")
            .to_string(),
        project_path: fold_optional_path(normalized.project_path.as_deref()),
        started_at: normalized.started_at.clone(),
        updated_at: normalized.updated_at.clone(),
        source_locator: fold_optional_path(normalized.source_locator.as_deref()),
        source_fingerprint: normalized.source_fingerprint.clone(),
        missing: false,
        created_at: now.to_string(),
        imported_at: now.to_string(),
        execution_origin,
        execution_purpose,
        user_visible,
        last_cwd: derive_session_last_cwd(&normalized.turns),
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ConversationRecordTables {
    pub(super) sessions: &'static str,
    pub(super) session_project_path_expr: &'static str,
    pub(super) session_metadata_expr: &'static str,
    pub(super) turns: &'static str,
    pub(super) parts: &'static str,
    pub(super) part_columns: &'static str,
    pub(super) session_id_prefix: &'static str,
    pub(super) turn_id_prefix: &'static str,
}

impl ConversationRecordKind {
    pub(super) fn tables(self) -> ConversationRecordTables {
        match self {
            ConversationRecordKind::Session => ConversationRecordTables {
                sessions: "conversation_sessions",
                session_project_path_expr: "s.project_path",
                session_metadata_expr: "s.execution_origin, s.execution_purpose, s.user_visible, s.last_cwd,",
                turns: "conversation_turns",
                parts: "conversation_parts",
                part_columns: "p.turn_id, p.part_index, p.kind, p.content, p.attrs_json, p.translated_text",
                session_id_prefix: "conversation-session-",
                turn_id_prefix: "conversation-turn-",
            },
            ConversationRecordKind::Web => ConversationRecordTables {
                sessions: "web_record_sessions",
                session_project_path_expr: "NULL",
                session_metadata_expr: "NULL AS execution_origin, NULL AS execution_purpose, 1 AS user_visible, NULL AS last_cwd,",
                turns: "web_record_turns",
                parts: "web_record_parts",
                part_columns: "p.turn_id, p.part_index, p.kind, p.content, NULL AS attrs_json, p.translated_text",
                session_id_prefix: "web-record-session-",
                turn_id_prefix: "web-record-turn-",
            },
        }
    }
}

pub(super) fn compact_whitespace(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub(super) fn normalize_query(query: Option<&str>) -> Option<String> {
    query
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_lowercase)
}

pub(super) fn first_line(text: &str) -> String {
    let line = text
        .lines()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("Untitled question");
    let trimmed = line.trim();
    if trimmed.chars().count() > 96 {
        trimmed.chars().take(96).collect()
    } else {
        trimmed.to_string()
    }
}

pub(super) fn search_question_title_from_turns(
    question: &ConversationQuestion,
    turns: &[ConversationTurn],
) -> String {
    question
        .title
        .clone()
        .filter(|title| !title.trim().is_empty())
        .or_else(|| {
            turns
                .iter()
                .map(|turn| turn.prompt.as_str())
                .find(|text| !text.trim().is_empty())
                .map(first_line)
        })
        .unwrap_or_else(|| "Untitled question".to_string())
}

pub(super) fn stable_id(prefix: &str, parts: &[&str]) -> String {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update(part.as_bytes());
        hasher.update(b"\0");
    }
    format!("{prefix}-{:x}", hasher.finalize())
}

pub(super) async fn begin_immediate_with_retry(
    pool: &SqlitePool,
    max_attempts: usize,
) -> StoreResult<sqlx::Transaction<'static, sqlx::Sqlite>> {
    let mut attempts = 0;
    loop {
        attempts += 1;
        match crate::backend::store::begin_immediate(pool).await {
            Ok(tx) => return Ok(tx),
            Err(err) if attempts < max_attempts && is_retryable_db_lock_err(&err) => {
                let delay = std::time::Duration::from_millis(50 + (attempts as u64) * 80);
                tokio::time::sleep(delay).await;
            }
            Err(err) => return Err(err),
        }
    }
}

pub(super) fn is_retryable_db_lock_err(err: &StoreError) -> bool {
    let msg = err.to_string();
    msg.contains("database is locked")
        || msg.contains("code: 517")
        || msg.contains("code: 5")
        || msg.contains("busy")
        || msg.contains("Resource temporarily unavailable")
        || msg.contains("timed out")
}

pub(super) fn derive_session_last_cwd(
    turns: &[crate::backend::domain::NormalizedConversationTurn],
) -> Option<String> {
    let mut candidates = Vec::new();
    for turn in turns {
        if turn.user_text.trim().is_empty() {
            continue;
        }
        let ts = turn.ended_at.as_deref().or(turn.started_at.as_deref());
        for (part_idx, part) in turn.parts.iter().enumerate() {
            if let Some(ref cwd) = part.cwd {
                let trimmed = cwd.trim();
                if !trimmed.is_empty() {
                    candidates.push((ts, turn.turn_index, part_idx, trimmed.to_string()));
                }
            }
        }
    }
    if candidates.is_empty() {
        for turn in turns {
            let ts = turn.ended_at.as_deref().or(turn.started_at.as_deref());
            for (part_idx, part) in turn.parts.iter().enumerate() {
                if let Some(ref cwd) = part.cwd {
                    let trimmed = cwd.trim();
                    if !trimmed.is_empty() {
                        candidates.push((ts, turn.turn_index, part_idx, trimmed.to_string()));
                    }
                }
            }
        }
    }
    candidates.sort_by(|a, b| {
        b.0.cmp(&a.0)
            .then_with(|| b.1.cmp(&a.1))
            .then_with(|| b.2.cmp(&a.2))
    });
    candidates
        .into_iter()
        .next()
        .map(|(_, _, _, cwd)| fold_path_for_storage(&cwd))
        .filter(|cwd| !cwd.is_empty())
}

pub(super) async fn update_conversation_session_last_cwd_sqlx_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    tenant_id: &str,
    session_id: &str,
    last_cwd: Option<&str>,
) -> StoreResult<()> {
    sqlx::query("UPDATE conversation_sessions SET last_cwd = ?1 WHERE tenant_id = ?2 AND id = ?3")
        .bind(last_cwd)
        .bind(tenant_id)
        .bind(session_id)
        .execute(&mut **tx)
        .await
        .map_err(StoreError::external)?;
    Ok(())
}
