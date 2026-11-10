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
    pub content: String,
    pub adapter_id: String,
    pub source_id: String,
    pub project_path: String,
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

pub async fn load_conversation_search_index_documents_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
) -> StoreResult<Vec<ConversationSearchIndexDocumentRow>> {
    let mut documents = Vec::new();
    for tables in [SearchDocumentTables::session(), SearchDocumentTables::web()] {
        let turn_sql = format!(
            r#"
            SELECT s.id AS session_id, t.id AS turn_id, t.turn_index AS turn_index, t.prompt AS prompt,
                   t.process AS process, t.response AS response,
                   s.adapter_id AS adapter_id, s.source_id AS source_id,
                   {project_path} AS project_path
            FROM {sessions} s
            JOIN {turns} t ON t.tenant_id = s.tenant_id AND t.session_id = s.id
            WHERE s.tenant_id = ?1 AND s.missing = 0
            ORDER BY s.id ASC, t.turn_index ASC, t.id ASC
            "#,
            sessions = tables.sessions,
            turns = tables.turns,
            project_path = tables.project_path,
        );
        let turn_rows = sqlx::query_as::<_, SearchIndexTurnRow>(AssertSqlSafe(turn_sql))
            .bind(tenant_id)
            .fetch_all(pool)
            .await?;

        let part_sql = format!(
            r#"
            SELECT p.turn_id, p.part_index, p.kind, p.content, {attrs_expr} AS attrs_json, p.translated_text
            FROM {parts} p
            JOIN {turns} t ON t.tenant_id = p.tenant_id AND t.id = p.turn_id
            JOIN {sessions} s ON s.tenant_id = t.tenant_id AND s.id = t.session_id
            WHERE p.tenant_id = ?1 AND s.missing = 0
            ORDER BY p.turn_id, p.part_index
            "#,
            parts = tables.parts,
            turns = tables.turns,
            sessions = tables.sessions,
            attrs_expr = tables.attrs_expr,
        );
        let raw_part_rows = sqlx::query_as::<
            _,
            (
                String,
                i64,
                String,
                Option<String>,
                Option<String>,
                Option<String>,
            ),
        >(AssertSqlSafe(part_sql))
        .bind(tenant_id)
        .fetch_all(pool)
        .await?;

        let mut parts_by_turn =
            BTreeMap::<String, Vec<crate::backend::domain::conversations::triad::TriadPart>>::new();
        for (turn_id, part_index, kind, content, attrs_json, translated_text) in raw_part_rows {
            parts_by_turn.entry(turn_id).or_default().push(
                crate::backend::domain::conversations::triad::TriadPart {
                    part_index,
                    kind,
                    content,
                    attrs_json,
                    translated_text,
                },
            );
        }

        let mut turns_by_session = BTreeMap::<String, Vec<SearchIndexTurnRow>>::new();
        for turn in turn_rows {
            turns_by_session
                .entry(turn.session_id.clone())
                .or_default()
                .push(turn);
        }

        for (_, mut session_turns) in turns_by_session {
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

            for group in groups {
                let question_id = group.question_id;
                let question_title = group
                    .turns
                    .first()
                    .map(|turn| search_question_title(None, &turn.prompt))
                    .unwrap_or_else(|| "Untitled question".to_string());

                for turn in group.turns {
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
                    // 与 FTS5 派生索引共用同一份语料：只放行提问、最终回答、
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
                        documents.push(ConversationSearchIndexDocumentRow {
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
                            content: document.text,
                            adapter_id: turn.adapter_id.clone(),
                            source_id: turn.source_id.clone(),
                            project_path: turn.project_path.clone().unwrap_or_default(),
                        });
                    }
                }
            }
        }
    }
    Ok(documents)
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

struct SearchDocumentTables {
    record_kind: &'static str,
    prefix: &'static str,
    question_prefix: &'static str,
    sessions: &'static str,
    turns: &'static str,
    parts: &'static str,
    project_path: &'static str,
    attrs_expr: &'static str,
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
