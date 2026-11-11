use std::collections::BTreeMap;

use crate::backend::domain::conversations::search_corpus::current_search_corpus_policy;
use crate::backend::store::StoreResult;
use sqlx::{AssertSqlSafe, SqlitePool};

#[derive(Debug, Clone)]
pub struct ConversationSearchIndexDocumentRow {
    pub document_kind: String,
    pub record_kind: String,
    pub session_id: String,
    pub question_id: String,
    pub turn_id: String,
    pub part_id: String,
    pub block_id: String,
    pub card_kind: String,
    pub semantic_role: String,
    pub question_title: String,
    /// 会话标题；Question/Card 文档留空，由 Session 文档承载。
    pub session_title: String,
    pub content: String,
    pub adapter_id: String,
    pub source_id: String,
    pub project_path: String,
    /// 子代理会话的可见父会话 ID；根会话为空。
    pub parent_session_id: String,
}

#[derive(Debug, sqlx::FromRow)]
struct SearchIndexTurnRow {
    session_id: String,
    turn_id: String,
    turn_index: i64,
    prompt: String,
    process: Option<String>,
    response: Option<String>,
    adapter_id: String,
    source_id: String,
    project_path: Option<String>,
}

#[derive(Debug, sqlx::FromRow)]
struct SearchIndexSessionRow {
    session_id: String,
    title: String,
    external_id: String,
    adapter_id: String,
    source_id: String,
    project_path: String,
    parent_session_id: String,
}

/// 整库装载（受控整代重建使用）。
pub async fn load_conversation_search_index_documents_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
) -> StoreResult<Vec<ConversationSearchIndexDocumentRow>> {
    load_conversation_search_index_documents_inner(pool, tenant_id, None).await
}

/// 只装载指定会话的文档（事实提交后的增量推进使用）。
///
/// 未列出的会话完全不被读取，因此日常同步的索引成本只与实际变化的会话数成正比。
pub async fn load_conversation_search_index_documents_for_sessions_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    session_ids: &[String],
) -> StoreResult<Vec<ConversationSearchIndexDocumentRow>> {
    load_conversation_search_index_documents_inner(pool, tenant_id, Some(session_ids)).await
}

async fn load_conversation_search_index_documents_inner(
    pool: &SqlitePool,
    tenant_id: &str,
    session_ids: Option<&[String]>,
) -> StoreResult<Vec<ConversationSearchIndexDocumentRow>> {
    let session_filter = session_ids.map(session_ids_json);
    let mut documents = Vec::new();
    for tables in [SearchDocumentTables::session(), SearchDocumentTables::web()] {
        let session_rows =
            load_sessions_sqlx(pool, tenant_id, tables, session_filter.as_deref()).await?;
        if session_rows.is_empty() {
            continue;
        }
        let turn_rows = load_turns_sqlx(pool, tenant_id, tables, session_filter.as_deref()).await?;
        let mut parts_by_turn =
            load_parts_by_turn_sqlx(pool, tenant_id, tables, session_filter.as_deref()).await?;

        let mut turns_by_session = BTreeMap::<String, Vec<SearchIndexTurnRow>>::new();
        for turn in turn_rows {
            turns_by_session
                .entry(turn.session_id.clone())
                .or_default()
                .push(turn);
        }

        for session in session_rows {
            let Some(mut session_turns) = turns_by_session.remove(&session.session_id) else {
                // 没有 Turn 的会话没有任何高信号正文，标题匹配仍由事实表承担。
                continue;
            };
            session_turns.sort_by(|a, b| {
                a.turn_index
                    .cmp(&b.turn_index)
                    .then_with(|| a.turn_id.cmp(&b.turn_id))
            });
            let groups = crate::backend::domain::conversations::group_turns_by_dynamic_projection(
                tables.question_prefix,
                session_turns,
                |turn: &SearchIndexTurnRow| &turn.prompt,
                |turn: &SearchIndexTurnRow| &turn.turn_id,
            );

            let mut session_documents = Vec::new();
            let mut first_prompt = String::new();
            for group in groups {
                let question_id = group.question_id;
                let question_title = group
                    .turns
                    .first()
                    .map(|turn| search_question_title(None, &turn.prompt))
                    .unwrap_or_else(|| "Untitled question".to_string());

                for turn in group.turns {
                    if first_prompt.is_empty() {
                        first_prompt = turn.prompt.clone();
                    }
                    let turn_parts = parts_by_turn.remove(&turn.turn_id).unwrap_or_default();
                    let restored_parts =
                        crate::backend::store::conversations::snapshot::restore_turn_parts(
                            &turn.adapter_id,
                            tables.prefix,
                            &turn.turn_id,
                            &turn.prompt,
                            turn.process.as_deref(),
                            turn.response.as_deref(),
                            &turn_parts,
                        );
                    // Question/Card 与 Session 文档共用同一份语料：只放行提问、最终回答、
                    // 变更文件路径与命令语义标签（ADR-0023）。
                    let corpus_documents = current_search_corpus_policy().extract(
                        &crate::backend::domain::conversations::search_corpus::CorpusTurnFacts {
                            turn_id: &turn.turn_id,
                            prompt: &turn.prompt,
                            response: turn.response.as_deref(),
                            parts: &restored_parts,
                        },
                    );

                    for document in corpus_documents {
                        let (document_kind, card_kind, semantic_role) =
                            corpus_signal_scoping(document.signal);
                        session_documents.push(ConversationSearchIndexDocumentRow {
                            document_kind,
                            record_kind: tables.record_kind.to_string(),
                            session_id: turn.session_id.clone(),
                            question_id: question_id.clone(),
                            turn_id: turn.turn_id.clone(),
                            part_id: String::new(),
                            block_id: document.anchor_id(),
                            card_kind,
                            semantic_role,
                            question_title: question_title.clone(),
                            session_title: String::new(),
                            content: document.text,
                            adapter_id: turn.adapter_id.clone(),
                            source_id: turn.source_id.clone(),
                            project_path: turn.project_path.clone().unwrap_or_default(),
                            parent_session_id: session.parent_session_id.clone(),
                        });
                    }
                }
            }

            let title = if session.title.trim().is_empty() {
                search_question_title(None, &first_prompt)
            } else {
                session.title.clone()
            };
            documents.push(session_document_row(
                &session,
                tables.record_kind,
                &title,
                &session_documents,
            ));
            documents.extend(session_documents);
        }
    }
    Ok(documents)
}

/// 一个会话一行：标题可搜 + 该会话高信号正文的聚合。
///
/// 聚合正文让列表检索能"按会话命中"而不炸出成百上千条 Turn 级命中；
/// 单文档大小做了上限，避免超长会话把一代索引撑成单个巨型段。
fn session_document_row(
    session: &SearchIndexSessionRow,
    record_kind: &str,
    title: &str,
    session_documents: &[ConversationSearchIndexDocumentRow],
) -> ConversationSearchIndexDocumentRow {
    let mut aggregate = String::new();
    for document in session_documents {
        if document.content.is_empty() {
            continue;
        }
        if !aggregate.is_empty() {
            aggregate.push('\n');
        }
        aggregate.push_str(&document.content);
        if aggregate.len() >= MAX_SESSION_DOCUMENT_BYTES {
            break;
        }
    }
    truncate_on_char_boundary(&mut aggregate, MAX_SESSION_DOCUMENT_BYTES);
    ConversationSearchIndexDocumentRow {
        document_kind: "session".to_string(),
        record_kind: record_kind.to_string(),
        session_id: session.session_id.clone(),
        question_id: String::new(),
        turn_id: String::new(),
        part_id: String::new(),
        block_id: format!("{}:session", session.session_id),
        card_kind: String::new(),
        semantic_role: "session".to_string(),
        question_title: String::new(),
        session_title: title.to_string(),
        content: aggregate,
        adapter_id: session.adapter_id.clone(),
        source_id: session.source_id.clone(),
        project_path: session.project_path.clone(),
        parent_session_id: session.parent_session_id.clone(),
    }
}

/// 单个会话聚合正文的上限。超过部分由 Turn 级文档继续承载可检索性。
const MAX_SESSION_DOCUMENT_BYTES: usize = 512 * 1024;

fn truncate_on_char_boundary(text: &mut String, max_bytes: usize) {
    if text.len() <= max_bytes {
        return;
    }
    let mut end = max_bytes;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    text.truncate(end);
}

async fn load_sessions_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    tables: SearchDocumentTables,
    session_filter: Option<&str>,
) -> StoreResult<Vec<SearchIndexSessionRow>> {
    let sql = format!(
        r#"
        SELECT s.id AS session_id, s.title AS title, s.external_id AS external_id,
               s.adapter_id AS adapter_id, s.source_id AS source_id,
               {project_path} AS project_path,
               {parent_session_id} AS parent_session_id
        FROM {sessions} s
        WHERE s.tenant_id = ?1 AND s.missing = 0
          AND (?2 IS NULL OR s.id IN (SELECT value FROM json_each(?2)))
        ORDER BY s.id ASC
        "#,
        sessions = tables.sessions,
        project_path = tables.project_path,
        parent_session_id = tables.parent_session_id,
    );
    Ok(
        sqlx::query_as::<_, SearchIndexSessionRow>(AssertSqlSafe(sql))
            .bind(tenant_id)
            .bind(session_filter)
            .fetch_all(pool)
            .await?,
    )
}

async fn load_turns_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    tables: SearchDocumentTables,
    session_filter: Option<&str>,
) -> StoreResult<Vec<SearchIndexTurnRow>> {
    let sql = format!(
        r#"
        SELECT s.id AS session_id, t.id AS turn_id, t.turn_index AS turn_index, t.prompt AS prompt,
               t.process AS process, t.response AS response,
               s.adapter_id AS adapter_id, s.source_id AS source_id,
               {project_path} AS project_path
        FROM {sessions} s
        JOIN {turns} t ON t.tenant_id = s.tenant_id AND t.session_id = s.id
        WHERE s.tenant_id = ?1 AND s.missing = 0
          AND (?2 IS NULL OR s.id IN (SELECT value FROM json_each(?2)))
        ORDER BY s.id ASC, t.turn_index ASC, t.id ASC
        "#,
        sessions = tables.sessions,
        turns = tables.turns,
        project_path = tables.project_path,
    );
    Ok(sqlx::query_as::<_, SearchIndexTurnRow>(AssertSqlSafe(sql))
        .bind(tenant_id)
        .bind(session_filter)
        .fetch_all(pool)
        .await?)
}

type RawPartRow = (
    String,
    i64,
    String,
    Option<String>,
    Option<String>,
    Option<String>,
);

async fn load_parts_by_turn_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    tables: SearchDocumentTables,
    session_filter: Option<&str>,
) -> StoreResult<BTreeMap<String, Vec<crate::backend::domain::conversations::triad::TriadPart>>> {
    // 轮次驱动 + CROSS JOIN 固定连接顺序：零件驱动会退化成"扫描本租户全部零件"。
    let sql = format!(
        r#"
        SELECT p.turn_id, p.part_index, p.kind, p.content, {attrs_expr} AS attrs_json, p.translated_text
        FROM {sessions} s
        CROSS JOIN {turns} t ON t.tenant_id = s.tenant_id AND t.session_id = s.id
        CROSS JOIN {parts} p ON p.tenant_id = t.tenant_id AND p.turn_id = t.id
        WHERE s.tenant_id = ?1 AND s.missing = 0
          AND (?2 IS NULL OR s.id IN (SELECT value FROM json_each(?2)))
        ORDER BY p.turn_id, p.part_index
        "#,
        parts = tables.parts,
        turns = tables.turns,
        sessions = tables.sessions,
        attrs_expr = tables.attrs_expr,
    );
    let raw_part_rows = sqlx::query_as::<_, RawPartRow>(AssertSqlSafe(sql))
        .bind(tenant_id)
        .bind(session_filter)
        .fetch_all(pool)
        .await?;

    let mut parts_by_turn = BTreeMap::new();
    for (turn_id, part_index, kind, content, attrs_json, translated_text) in raw_part_rows {
        parts_by_turn.entry(turn_id).or_insert_with(Vec::new).push(
            crate::backend::domain::conversations::triad::TriadPart {
                part_index,
                kind,
                content,
                attrs_json,
                translated_text,
            },
        );
    }
    Ok(parts_by_turn)
}

/// 索引不可用时的正确性回退：在已提交事实上按同一高信号语料做子串匹配。
///
/// 两段式：先用事实列做一次超集筛（正文语料只可能由这些列派生），
/// 再用 [`current_search_corpus_policy`] 重算文档逐条确认。
/// 因此回退的命中口径与索引完全一致，不会退化成"仅标题匹配"。
pub async fn search_session_ids_by_corpus_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    record_kind: crate::backend::domain::ConversationRecordKind,
    needle: &str,
    aggregate_to_parent: bool,
) -> StoreResult<Vec<String>> {
    let needle = needle.trim().to_lowercase();
    if needle.is_empty() {
        return Ok(Vec::new());
    }
    let tables = match record_kind {
        crate::backend::domain::ConversationRecordKind::Session => SearchDocumentTables::session(),
        crate::backend::domain::ConversationRecordKind::Web => SearchDocumentTables::web(),
    };
    let candidates =
        load_corpus_candidate_session_ids_sqlx(pool, tenant_id, tables, &needle).await?;
    if candidates.is_empty() {
        return Ok(Vec::new());
    }
    let documents =
        load_conversation_search_index_documents_for_sessions_sqlx(pool, tenant_id, &candidates)
            .await?;
    let mut matched = std::collections::BTreeSet::new();
    for document in documents {
        if document.record_kind == tables.record_kind
            && document.content.to_lowercase().contains(&needle)
        {
            matched.insert(document.session_id);
        }
    }
    if aggregate_to_parent && !matched.is_empty() {
        // 与索引侧的归集规则保持一致：子代理会话的命中归到它的可见父会话。
        let child_ids = serde_json::Value::Array(
            matched
                .iter()
                .map(|id| serde_json::Value::String(id.clone()))
                .collect(),
        )
        .to_string();
        let parents: Vec<String> = sqlx::query_scalar(
            r#"
            SELECT json_extract(execution_purpose, '$.parent_session_id')
            FROM conversation_sessions
            WHERE tenant_id = ?1
              AND id IN (SELECT value FROM json_each(?2))
              AND execution_origin = 'subagent'
              AND json_extract(execution_purpose, '$.parent_session_id') IS NOT NULL
            "#,
        )
        .bind(tenant_id)
        .bind(child_ids)
        .fetch_all(pool)
        .await?;
        matched.extend(parents);
    }
    Ok(matched.into_iter().collect())
}

/// 事实侧候选：任何语料文档的正文都只能由这些列派生，因此这里是安全超集。
async fn load_corpus_candidate_session_ids_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
    tables: SearchDocumentTables,
    needle: &str,
) -> StoreResult<Vec<String>> {
    let attrs_match = if tables.attrs_expr == "NULL" {
        "0".to_string()
    } else {
        format!(
            "instr(lower(COALESCE({attrs}, '')), ?2) > 0",
            attrs = tables.attrs_expr
        )
    };
    let sql = format!(
        r#"
        SELECT DISTINCT t.session_id
        FROM {turns} t
        WHERE t.tenant_id = ?1
          AND (
              instr(lower(t.prompt), ?2) > 0
              OR instr(lower(COALESCE(t.response, '')), ?2) > 0
          )
        UNION
        SELECT DISTINCT t.session_id
        FROM {turns} t
        CROSS JOIN {parts} p ON p.tenant_id = t.tenant_id AND p.turn_id = t.id
        WHERE t.tenant_id = ?1
          AND (instr(lower(COALESCE(p.content, '')), ?2) > 0 OR {attrs_match})
        "#,
        turns = tables.turns,
        parts = tables.parts,
    );
    Ok(sqlx::query_scalar::<_, String>(AssertSqlSafe(sql))
        .bind(tenant_id)
        .bind(needle)
        .fetch_all(pool)
        .await?)
}

fn session_ids_json(session_ids: &[String]) -> String {
    serde_json::Value::Array(
        session_ids
            .iter()
            .map(|id| serde_json::Value::String(id.clone()))
            .collect(),
    )
    .to_string()
}

/// 把语料信号映射成独立索引的文档筛选维度。
///
/// 既有的 `include_questions` / `card_kinds` / `semantic_roles` 过滤语义保持可用：
/// 提问仍归入 `question`，其余三类归入 `card` 并沿用适配器作用域下的卡片种类。
fn corpus_signal_scoping(
    signal: crate::backend::domain::conversations::search_corpus::SearchCorpusSignal,
) -> (String, String, String) {
    use crate::backend::domain::conversations::search_corpus::SearchCorpusSignal;

    // 卡片种类统一用信号名，不带适配器前缀：两个检索后端必须给出同一套分类与筛选维度。
    match signal {
        SearchCorpusSignal::Prompt => ("question".to_string(), String::new(), String::new()),
        SearchCorpusSignal::Answer => (
            "card".to_string(),
            SearchCorpusSignal::Answer.as_str().to_string(),
            SearchCorpusSignal::Answer.as_str().to_string(),
        ),
        SearchCorpusSignal::ChangedFile => (
            "card".to_string(),
            SearchCorpusSignal::ChangedFile.as_str().to_string(),
            SearchCorpusSignal::ChangedFile.as_str().to_string(),
        ),
        SearchCorpusSignal::CommandLabel => (
            "card".to_string(),
            SearchCorpusSignal::CommandLabel.as_str().to_string(),
            SearchCorpusSignal::CommandLabel.as_str().to_string(),
        ),
    }
}

#[derive(Clone, Copy)]
struct SearchDocumentTables {
    record_kind: &'static str,
    prefix: &'static str,
    question_prefix: &'static str,
    sessions: &'static str,
    turns: &'static str,
    parts: &'static str,
    project_path: &'static str,
    attrs_expr: &'static str,
    parent_session_id: &'static str,
}

impl SearchDocumentTables {
    fn session() -> Self {
        Self {
            record_kind: "session",
            prefix: "conversation-part",
            question_prefix: "conversation-question",
            sessions: "conversation_sessions",
            turns: "conversation_turns",
            parts: "conversation_parts",
            project_path: "COALESCE(s.project_path, '')",
            attrs_expr: "p.attrs_json",
            parent_session_id: "CASE WHEN s.execution_origin = 'subagent' \
                THEN COALESCE(json_extract(s.execution_purpose, '$.parent_session_id'), '') \
                ELSE '' END",
        }
    }

    fn web() -> Self {
        Self {
            record_kind: "web",
            prefix: "web-record-part",
            question_prefix: "web-record-question",
            sessions: "web_record_sessions",
            turns: "web_record_turns",
            parts: "web_record_parts",
            project_path: "''",
            attrs_expr: "NULL",
            parent_session_id: "''",
        }
    }
}

fn search_question_title(title: Option<String>, question_text: &str) -> String {
    title
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| {
            question_text
                .lines()
                .find(|line| !line.trim().is_empty())
                .unwrap_or("Untitled question")
                .trim()
                .to_string()
        })
}
