use super::prelude::*;

pub(super) fn read_codex_sessions(
    source: &ConversationSource,
) -> AppResult<Vec<NormalizedConversationSession>> {
    let root = crate::backend::path_utils::expand_path(&source.location)?;
    let db_path = if root.is_dir() {
        root.join("state_5.sqlite")
    } else {
        root.clone()
    };
    if !db_path.is_file() {
        return Ok(Vec::new());
    }
    let conn = Connection::open(&db_path).map_err(|error| error.to_string())?;
    let columns = table_columns(&conn, "threads")?;
    if columns.is_empty() {
        return Ok(Vec::new());
    }
    let id_col = pick_column(&columns, &["id", "thread_id", "session_id"])
        .ok_or_else(|| "Codex threads table has no id column".to_string())?;
    let rollout_col = pick_column(
        &columns,
        &["rollout_path", "path", "file_path", "jsonl_path"],
    )
    .ok_or_else(|| "Codex threads table has no rollout path column".to_string())?;
    let title_col = pick_column(&columns, &["title", "name"]);
    let updated_col = pick_column(
        &columns,
        &["updated_at", "last_updated_at", "mtime", "created_at"],
    );
    let query = format!(
        "SELECT {id_col}, {rollout_col}, {} , {} FROM threads ORDER BY rowid DESC LIMIT 500",
        title_col.unwrap_or("NULL"),
        updated_col.unwrap_or("NULL")
    );
    let mut stmt = conn.prepare(&query).map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                required_cell_string(row, 0)?,
                optional_cell_string(row, 1)?,
                optional_cell_string(row, 2)?,
                optional_cell_string(row, 3)?,
            ))
        })
        .map_err(|error| error.to_string())?;
    let mut sessions = Vec::new();
    for row in rows {
        let (external_id, rollout_path, title, updated_at) =
            row.map_err(|error| error.to_string())?;
        let Some(rollout_path) = rollout_path else {
            continue;
        };
        let path = crate::backend::path_utils::expand_path(&rollout_path)
            .unwrap_or_else(|_| PathBuf::from(&rollout_path));
        if !path.is_file() {
            continue;
        }
        let text = fs::read_to_string(&path).map_err(|error| error.to_string())?;
        let turns = parse_jsonl_conversation(&text, ParserFlavor::Codex)?;
        if turns.is_empty() {
            continue;
        }
        sessions.push(NormalizedConversationSession {
            external_id,
            title,
            project_path: infer_project_path_from_turns(&turns),
            started_at: turns.first().and_then(|turn| turn.started_at.clone()),
            updated_at,
            source_locator: Some(path.to_string_lossy().to_string()),
            source_fingerprint: Some(hash_bytes(text.as_bytes())),
            turns,
        });
    }
    Ok(sessions)
}

pub(super) fn read_opencode_sessions(
    source: &ConversationSource,
) -> AppResult<Vec<NormalizedConversationSession>> {
    let db_path = crate::backend::path_utils::expand_path(&source.location)?;
    if !db_path.is_file() {
        return Ok(Vec::new());
    }
    let conn = Connection::open(&db_path).map_err(|error| error.to_string())?;
    let session_columns = table_columns(&conn, "session")?;
    let message_columns = table_columns(&conn, "message")?;
    let part_columns = table_columns(&conn, "part")?;
    if session_columns.is_empty() || message_columns.is_empty() || part_columns.is_empty() {
        return Ok(Vec::new());
    }

    let session_id_col = pick_column(&session_columns, &["id", "session_id"])
        .ok_or_else(|| "OpenCode session table has no id column".to_string())?;
    let session_title_col = pick_column(&session_columns, &["title", "name"]);
    let project_col = pick_column(
        &session_columns,
        &["project", "project_path", "cwd", "path"],
    );
    let updated_col = pick_column(
        &session_columns,
        &["updated_at", "time_updated", "timeUpdated", "created_at"],
    );
    let query = format!(
        "SELECT {session_id_col}, {}, {}, {} FROM session ORDER BY rowid DESC LIMIT 500",
        session_title_col.unwrap_or("NULL"),
        project_col.unwrap_or("NULL"),
        updated_col.unwrap_or("NULL")
    );
    let mut stmt = conn.prepare(&query).map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                required_cell_string(row, 0)?,
                optional_cell_string(row, 1)?,
                optional_cell_string(row, 2)?,
                optional_cell_string(row, 3)?,
            ))
        })
        .map_err(|error| error.to_string())?;

    let source_fingerprint = hash_file(&db_path).ok();
    let mut sessions = Vec::new();
    for row in rows {
        let (external_id, title, project_path, updated_at) =
            row.map_err(|error| error.to_string())?;
        let turns = read_opencode_turns(&conn, &message_columns, &part_columns, &external_id)?;
        if turns.is_empty() {
            continue;
        }
        sessions.push(NormalizedConversationSession {
            external_id,
            title,
            project_path,
            started_at: turns.first().and_then(|turn| turn.started_at.clone()),
            updated_at,
            source_locator: Some(db_path.to_string_lossy().to_string()),
            source_fingerprint: source_fingerprint.clone(),
            turns,
        });
    }
    Ok(sessions)
}

fn read_opencode_turns(
    conn: &Connection,
    message_columns: &[String],
    part_columns: &[String],
    session_id: &str,
) -> AppResult<Vec<NormalizedConversationTurn>> {
    let part_rows_by_message = read_opencode_part_rows(conn, part_columns, session_id)?;
    let msg_id_col = pick_column(message_columns, &["id", "message_id"])
        .ok_or_else(|| "OpenCode message table has no id column".to_string())?;
    let msg_session_col = pick_column(message_columns, &["session_id", "sessionID", "session"])
        .ok_or_else(|| "OpenCode message table has no session id column".to_string())?;
    let role_col = pick_column(message_columns, &["role", "author"]);
    let time_col = pick_column(
        message_columns,
        &["created_at", "time_created", "timeCreated", "time"],
    );
    let data_col = pick_column(message_columns, &["data", "json", "metadata"]);
    let msg_query = format!(
        "SELECT {msg_id_col}, {}, {}, {} FROM message WHERE {msg_session_col} = ?1 ORDER BY rowid ASC",
        role_col.unwrap_or("NULL"),
        time_col.unwrap_or("NULL"),
        data_col.unwrap_or("NULL")
    );
    let mut stmt = conn
        .prepare(&msg_query)
        .map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map(params![session_id], |row| {
            Ok((
                required_cell_string(row, 0)?,
                optional_cell_string(row, 1)?,
                optional_cell_string(row, 2)?,
                optional_cell_string(row, 3)?,
            ))
        })
        .map_err(|error| error.to_string())?;

    let mut turns = Vec::new();
    let mut current: Option<NormalizedConversationTurn> = None;
    for row in rows {
        let (message_id, role, timestamp, data_json) = row.map_err(|error| error.to_string())?;
        let data_value = data_json
            .as_deref()
            .and_then(|text| serde_json::from_str::<Value>(text).ok());
        let message_role = role
            .or_else(|| {
                data_value
                    .as_ref()
                    .and_then(|value| value_field_any_as_string(value, &["role", "author"]))
            })
            .unwrap_or_default();
        let timestamp = timestamp.or_else(|| {
            data_value
                .as_ref()
                .and_then(|value| value_field_any_as_string(value, &["time", "created_at"]))
        });
        let parts = part_rows_by_message
            .get(&message_id)
            .map(|rows| normalize_opencode_parts(rows, message_role.as_str()))
            .unwrap_or_default();
        if message_role == "user" {
            let user_text = parts
                .iter()
                .filter(|part| part.role == ConversationPartRole::User)
                .filter(|part| part.kind == ConversationPartKind::Text)
                .filter_map(|part| part.text.clone())
                .collect::<Vec<_>>()
                .join("\n\n");
            if user_text.trim().is_empty() {
                continue;
            }
            if let Some(turn) = current.take() {
                turns.push(turn);
            }
            current = Some(NormalizedConversationTurn {
                external_id: message_id,
                turn_index: turns.len() as i64,
                user_text,
                title: None,
                started_at: timestamp,
                ended_at: None,
                parts: parts
                    .into_iter()
                    .filter(|part| part.role != ConversationPartRole::User)
                    .collect(),
            });
        } else if let Some(turn) = current.as_mut() {
            turn.parts.extend(parts);
            turn.ended_at = timestamp;
        }
    }
    if let Some(turn) = current {
        turns.push(turn);
    }
    Ok(turns)
}

#[derive(Debug, Clone)]
pub(super) struct OpenCodePartRow {
    pub(super) kind: String,
    pub(super) text: String,
    pub(super) command: Option<String>,
    pub(super) cwd: Option<String>,
    pub(super) status: Option<String>,
    pub(super) exit_code: Option<i32>,
    pub(super) metadata_json: Option<String>,
    pub(super) ignored: bool,
}

fn read_opencode_part_rows(
    conn: &Connection,
    columns: &[String],
    session_id: &str,
) -> AppResult<BTreeMap<String, Vec<OpenCodePartRow>>> {
    let session_col = pick_column(columns, &["session_id", "sessionID", "session"]);
    let message_col = pick_column(columns, &["message_id", "messageID", "message"])
        .ok_or_else(|| "OpenCode part table has no message id column".to_string())?;
    let type_col = pick_column(columns, &["type", "kind"]).unwrap_or("NULL");
    let text_col = pick_column(columns, &["text", "content", "output"]).unwrap_or("NULL");
    let data_col = pick_column(columns, &["data", "json", "metadata"]).unwrap_or("NULL");

    let query = if let Some(session_col) = session_col {
        format!(
            "SELECT {message_col}, {type_col}, {text_col}, {data_col} FROM part WHERE {session_col} = ?1 ORDER BY rowid ASC"
        )
    } else {
        format!(
            "SELECT {message_col}, {type_col}, {text_col}, {data_col} FROM part ORDER BY rowid ASC"
        )
    };
    let mut stmt = conn.prepare(&query).map_err(|error| error.to_string())?;
    let mut rows = if session_col.is_some() {
        stmt.query(params![session_id])
            .map_err(|error| error.to_string())?
    } else {
        stmt.query([]).map_err(|error| error.to_string())?
    };

    let mut by_message = BTreeMap::<String, Vec<OpenCodePartRow>>::new();
    while let Some(row) = rows.next().map_err(|error| error.to_string())? {
        let message_id = required_cell_string(row, 0).map_err(|error| error.to_string())?;
        let kind = optional_cell_string(row, 1).map_err(|error| error.to_string())?;
        let text = optional_cell_string(row, 2).map_err(|error| error.to_string())?;
        let data_json = optional_cell_string(row, 3).map_err(|error| error.to_string())?;
        let data_value = data_json
            .as_deref()
            .and_then(|text| serde_json::from_str::<Value>(text).ok());
        let kind = kind
            .or_else(|| {
                data_value
                    .as_ref()
                    .and_then(|value| value_field_any_as_string(value, &["type", "kind"]))
            })
            .unwrap_or_else(|| "text".to_string());
        let text = text
            .or_else(|| {
                data_value
                    .as_ref()
                    .and_then(|value| opencode_part_text(&kind, value))
            })
            .unwrap_or_default();
        let command = data_value.as_ref().and_then(command_from_value);
        let cwd = data_value.as_ref().and_then(cwd_from_value);
        let status = data_value.as_ref().and_then(status_from_value);
        let exit_code = data_value.as_ref().and_then(exit_code_from_value);
        let metadata_json = data_value.as_ref().map(compact_json);
        let ignored = data_value.as_ref().is_some_and(is_ignored_content_value);
        by_message
            .entry(message_id)
            .or_default()
            .push(OpenCodePartRow {
                kind,
                text,
                command,
                cwd,
                status,
                exit_code,
                metadata_json,
                ignored,
            });
    }

    Ok(by_message)
}

fn table_columns(conn: &Connection, table: &str) -> AppResult<Vec<String>> {
    let mut stmt = conn
        .prepare(&format!("PRAGMA table_info({table})"))
        .map_err(|error| error.to_string())?;
    let rows = stmt
        .query_map([], |row| row.get::<_, String>(1))
        .map_err(|error| error.to_string())?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())
}

fn pick_column<'a>(columns: &'a [String], candidates: &[&str]) -> Option<&'a str> {
    candidates.iter().find_map(|candidate| {
        columns
            .iter()
            .find(|column| column.eq_ignore_ascii_case(candidate))
            .map(String::as_str)
    })
}

fn required_cell_string(row: &Row<'_>, index: usize) -> rusqlite::Result<String> {
    Ok(optional_cell_string(row, index)?.unwrap_or_default())
}

fn optional_cell_string(row: &Row<'_>, index: usize) -> rusqlite::Result<Option<String>> {
    match row.get_ref(index)? {
        ValueRef::Null => Ok(None),
        ValueRef::Integer(value) => Ok(Some(value.to_string())),
        ValueRef::Real(value) => {
            if value.fract() == 0.0 {
                Ok(Some(format!("{value:.0}")))
            } else {
                Ok(Some(value.to_string()))
            }
        }
        ValueRef::Text(value) => Ok(Some(String::from_utf8_lossy(value).to_string())),
        ValueRef::Blob(value) => Ok(Some(hash_bytes(value))),
    }
}
