use crate::backend::{domain::ConversationPart, store::StoreResult};
use sqlx::{AssertSqlSafe, SqlitePool};

#[derive(Debug, Clone)]
pub(crate) struct ConversationSearchIndexDocumentRow {
    pub(crate) document_kind: String,
    pub(crate) record_kind: String,
    pub(crate) session_id: String,
    pub(crate) question_id: String,
    pub(crate) turn_id: String,
    pub(crate) part_id: String,
    pub(crate) block_id: String,
    pub(crate) card_kind: String,
    pub(crate) semantic_role: String,
    pub(crate) question_title: String,
    pub(crate) content: String,
    pub(crate) adapter_id: String,
    pub(crate) source_id: String,
    pub(crate) project_path: String,
}

#[derive(Debug, sqlx::FromRow)]
struct SearchIndexQuestionDocumentRow {
    session_id: String,
    question_id: String,
    turn_id: String,
    question_title: Option<String>,
    user_text: String,
    adapter_id: String,
    source_id: String,
    project_path: Option<String>,
}

#[derive(Debug, sqlx::FromRow)]
struct SearchIndexPartDocumentRow {
    session_id: String,
    question_id: String,
    turn_id: String,
    part_id: String,
    question_title: Option<String>,
    user_text: String,
    part_index: i64,
    role: String,
    kind: String,
    text: Option<String>,
    language: Option<String>,
    command: Option<String>,
    cwd: Option<String>,
    status: Option<String>,
    exit_code: Option<i64>,
    metadata_json: Option<String>,
    content_card_json: Option<String>,
    translated_text: Option<String>,
    source_execution_id: Option<String>,
    command_label: Option<String>,
    adapter_id: String,
    source_id: String,
    project_path: Option<String>,
    card_kinds_json: String,
}

pub(crate) async fn load_conversation_search_index_documents_sqlx(
    pool: &SqlitePool,
    tenant_id: &str,
) -> StoreResult<Vec<ConversationSearchIndexDocumentRow>> {
    let mut documents = Vec::new();
    for tables in [SearchDocumentTables::session(), SearchDocumentTables::web()] {
        let question_sql = format!(
            r#"
            SELECT s.id AS session_id, q.id AS question_id, t.id AS turn_id, q.title AS question_title, t.user_text AS user_text,
                   s.adapter_id AS adapter_id, s.source_id AS source_id, {project_path} AS project_path
            FROM {sessions} s
            JOIN {questions} q ON q.tenant_id = s.tenant_id AND q.session_id = s.id
            JOIN {question_turns} qt ON qt.tenant_id = q.tenant_id AND qt.question_id = q.id
            JOIN {turns} t ON t.tenant_id = qt.tenant_id AND t.id = qt.turn_id
            WHERE s.tenant_id = ?1 AND s.missing = 0 AND t.missing = 0
            ORDER BY s.id, COALESCE((
                SELECT MIN(t_order.turn_index)
                FROM {question_turns} qt_order
                JOIN {turns} t_order
                  ON t_order.tenant_id = qt_order.tenant_id
                 AND t_order.id = qt_order.turn_id
                WHERE qt_order.tenant_id = q.tenant_id
                  AND qt_order.question_id = q.id
            ), 9223372036854775807), qt.turn_order
            "#,
            sessions = tables.sessions,
            questions = tables.questions,
            question_turns = tables.question_turns,
            turns = tables.turns,
            project_path = tables.project_path,
        );
        for row in sqlx::query_as::<_, SearchIndexQuestionDocumentRow>(AssertSqlSafe(question_sql))
            .bind(tenant_id)
            .fetch_all(pool)
            .await?
        {
            let turn_id = row.turn_id;
            let user_text = row.user_text;
            let question_title = search_question_title(row.question_title, &user_text);
            documents.push(ConversationSearchIndexDocumentRow {
                document_kind: "question".to_string(),
                record_kind: tables.record_kind.to_string(),
                session_id: row.session_id,
                question_id: row.question_id,
                turn_id: turn_id.clone(),
                part_id: String::new(),
                block_id: format!("{turn_id}-question"),
                card_kind: String::new(),
                semantic_role: String::new(),
                question_title,
                content: user_text,
                adapter_id: row.adapter_id,
                source_id: row.source_id,
                project_path: row.project_path.unwrap_or_default(),
            });
        }

        let part_sql = format!(
            r#"
            SELECT s.id AS session_id, q.id AS question_id, t.id AS turn_id, p.id AS part_id, q.title AS question_title, t.user_text AS user_text,
                   p.part_index AS part_index, p.role AS role, p.kind AS kind, p.text AS text, p.language AS language,
                   p.command AS command, p.cwd AS cwd, p.status AS status, p.exit_code AS exit_code,
                   p.metadata_json AS metadata_json, p.content_card_json AS content_card_json,
                   p.translated_text AS translated_text, p.source_execution_id AS source_execution_id,
                   p.command_label AS command_label, s.adapter_id AS adapter_id, s.source_id AS source_id,
                   {project_path} AS project_path, COALESCE(a.card_kinds_json, '[]') AS card_kinds_json
            FROM {sessions} s
            JOIN {questions} q ON q.tenant_id = s.tenant_id AND q.session_id = s.id
            JOIN {question_turns} qt ON qt.tenant_id = q.tenant_id AND qt.question_id = q.id
            JOIN {turns} t ON t.tenant_id = qt.tenant_id AND t.id = qt.turn_id
            JOIN {parts} p ON p.tenant_id = t.tenant_id AND p.turn_id = t.id
            LEFT JOIN conversation_adapters a ON a.tenant_id = s.tenant_id AND a.id = s.adapter_id
            WHERE s.tenant_id = ?1 AND s.missing = 0 AND t.missing = 0
            ORDER BY s.id, COALESCE((
                SELECT MIN(t_order.turn_index)
                FROM {question_turns} qt_order
                JOIN {turns} t_order
                  ON t_order.tenant_id = qt_order.tenant_id
                 AND t_order.id = qt_order.turn_id
                WHERE qt_order.tenant_id = q.tenant_id
                  AND qt_order.question_id = q.id
            ), 9223372036854775807), qt.turn_order, p.part_index
            "#,
            sessions = tables.sessions,
            questions = tables.questions,
            question_turns = tables.question_turns,
            turns = tables.turns,
            parts = tables.parts,
            project_path = tables.project_path,
        );
        for row in sqlx::query_as::<_, SearchIndexPartDocumentRow>(AssertSqlSafe(part_sql))
            .bind(tenant_id)
            .fetch_all(pool)
            .await?
        {
            let part = map_search_part(&row)?;
            let card_kinds = serde_json::from_str::<
                Vec<crate::backend::domain::ConversationCardKindDefinition>,
            >(&row.card_kinds_json)
            .unwrap_or_default();
            let adapter_id = row.adapter_id.clone();
            let cards =
                crate::backend::domain::conversations::projection::project_conversation_content_cards(
                    &part,
                    &adapter_id,
                    &card_kinds,
                )?;
            let question_title = search_question_title(row.question_title.clone(), &row.user_text);
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
                    session_id: row.session_id.clone(),
                    question_id: row.question_id.clone(),
                    turn_id: row.turn_id.clone(),
                    part_id: part.id.clone(),
                    block_id: card.node_id,
                    card_kind,
                    semantic_role,
                    question_title: question_title.clone(),
                    content: card.body,
                    adapter_id: row.adapter_id.clone(),
                    source_id: row.source_id.clone(),
                    project_path: row.project_path.clone().unwrap_or_default(),
                });
            }
        }
    }
    Ok(documents)
}

fn map_search_part(row: &SearchIndexPartDocumentRow) -> StoreResult<ConversationPart> {
    Ok(ConversationPart {
        id: row.part_id.clone(),
        turn_id: row.turn_id.clone(),
        part_index: row.part_index,
        role: crate::backend::store::codec::decode_enum(row.role.clone())?,
        kind: crate::backend::store::codec::decode_enum(row.kind.clone())?,
        text: row.text.clone(),
        language: row.language.clone(),
        command: row.command.clone(),
        cwd: row.cwd.clone(),
        status: row.status.clone(),
        exit_code: row.exit_code.map(|v| v as i32),
        metadata_json: row.metadata_json.clone(),
        command_label: row.command_label.clone(),
        source_execution_id: row.source_execution_id.clone(),
        content_card: row
            .content_card_json
            .clone()
            .map(crate::backend::store::codec::decode_json)
            .transpose()?,
        translated_text: row.translated_text.clone(),
    })
}

struct SearchDocumentTables {
    record_kind: &'static str,
    sessions: &'static str,
    questions: &'static str,
    question_turns: &'static str,
    turns: &'static str,
    parts: &'static str,
    project_path: &'static str,
}

impl SearchDocumentTables {
    fn session() -> Self {
        Self {
            record_kind: "session",
            sessions: "conversation_sessions",
            questions: "conversation_questions",
            question_turns: "conversation_question_turns",
            turns: "conversation_turns",
            parts: "conversation_parts",
            project_path: "COALESCE(s.project_path, '')",
        }
    }

    fn web() -> Self {
        Self {
            record_kind: "web",
            sessions: "web_record_sessions",
            questions: "web_record_questions",
            question_turns: "web_record_question_turns",
            turns: "web_record_turns",
            parts: "web_record_parts",
            project_path: "''",
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
