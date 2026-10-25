use std::collections::BTreeMap;

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
    card_kinds_json: String,
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
                   {project_path} AS project_path, COALESCE(a.card_kinds_json, '[]') AS card_kinds_json
            FROM {sessions} s
            JOIN {turns} t ON t.tenant_id = s.tenant_id AND t.session_id = s.id
            LEFT JOIN conversation_adapters a ON a.tenant_id = s.tenant_id AND a.id = s.adapter_id
            WHERE s.tenant_id = ?1 AND s.missing = 0 AND t.missing = 0
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
            WHERE p.tenant_id = ?1 AND s.missing = 0 AND t.missing = 0
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
                    documents.push(ConversationSearchIndexDocumentRow {
                        document_kind: "question".to_string(),
                        record_kind: tables.record_kind.to_string(),
                        session_id: turn.session_id.clone(),
                        question_id: question_id.clone(),
                        turn_id: turn.turn_id.clone(),
                        part_id: String::new(),
                        block_id: format!("{}-question", turn.turn_id),
                        card_kind: String::new(),
                        semantic_role: String::new(),
                        question_title: question_title.clone(),
                        content: turn.prompt.clone(),
                        adapter_id: turn.adapter_id.clone(),
                        source_id: turn.source_id.clone(),
                        project_path: turn.project_path.clone().unwrap_or_default(),
                    });

                    let turn_parts = parts_by_turn.remove(&turn.turn_id).unwrap_or_default();
                    let process: Option<crate::backend::domain::conversations::triad::TurnProcess> =
                        turn.process
                            .as_deref()
                            .and_then(|s| serde_json::from_str(s).ok());
                    let restored_parts =
                        crate::backend::domain::conversations::triad::merge_turn_triad(
                            &turn.adapter_id,
                            tables.prefix,
                            &turn.turn_id,
                            &turn.prompt,
                            process.as_ref(),
                            turn.response.as_deref(),
                            &turn_parts,
                        );

                    let card_kinds = serde_json::from_str::<
                        Vec<crate::backend::domain::ConversationCardKindDefinition>,
                    >(&turn.card_kinds_json)
                    .unwrap_or_default();

                    for part in restored_parts {
                        let cards =
                            crate::backend::domain::conversations::projection::project_conversation_content_cards(
                                &part,
                                &turn.adapter_id,
                                &card_kinds,
                            )?;
                        for card in cards {
                            let card_kind = card.kind;
                            let semantic_role = card
                                .semantic_role
                                .or_else(|| {
                                    card_kind
                                        .rsplit_once('.')
                                        .map(|(_, value)| value.to_string())
                                })
                                .unwrap_or_default();
                            documents.push(ConversationSearchIndexDocumentRow {
                                document_kind: "card".to_string(),
                                record_kind: tables.record_kind.to_string(),
                                session_id: turn.session_id.clone(),
                                question_id: question_id.clone(),
                                turn_id: turn.turn_id.clone(),
                                part_id: part.id.clone(),
                                block_id: card.node_id,
                                card_kind,
                                semantic_role,
                                question_title: question_title.clone(),
                                content: card.body,
                                adapter_id: turn.adapter_id.clone(),
                                source_id: turn.source_id.clone(),
                                project_path: turn.project_path.clone().unwrap_or_default(),
                            });
                        }
                    }
                }
            }
        }
    }
    Ok(documents)
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
