use super::*;
use crate::backend::application::prelude::*;
use crate::backend::application::AppService;
use crate::backend::domain::conversations::{ConversationExportFormat, ConversationSource};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
#[derive(Debug, Serialize)]
pub(crate) struct ConversationRawQuestion {
    id: String,
    session_id: String,
    pub(crate) title: Option<String>,
    created_at: String,
    updated_at: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct ConversationRawExport<'a> {
    schema_version: u32,
    format: &'static str,
    record_kind: &'a str,
    source: &'a ConversationSource,
    pub(crate) session: &'a crate::backend::domain::ConversationSession,
    questions: Vec<ConversationRawQuestion>,
    question_turns: Vec<crate::backend::domain::ConversationQuestionTurn>,
    turns: Vec<crate::backend::domain::ConversationTurn>,
    parts: Vec<crate::backend::domain::ConversationPart>,
}

pub(crate) fn export_conversation_raw_json(
    detail: &crate::backend::domain::conversations::ConversationSessionDetail,
    source: &ConversationSource,
    question_ids: &[String],
    record_kind: &str,
) -> AppResult<String> {
    let selected = question_ids.iter().collect::<BTreeSet<_>>();
    let selected_questions = detail
        .questions
        .iter()
        .filter(|question| selected.is_empty() || selected.contains(&question.question.id))
        .collect::<Vec<_>>();
    let questions = selected_questions
        .iter()
        .map(|question| ConversationRawQuestion {
            id: question.question.id.clone(),
            session_id: question.question.session_id.clone(),
            title: question.question.title.clone(),
            created_at: question.question.created_at.clone(),
            updated_at: question.question.updated_at.clone(),
        })
        .collect();
    let question_turns = detail
        .questions
        .iter()
        .filter(|question| selected.is_empty() || selected.contains(&question.question.id))
        .flat_map(|question| question.question_turns.iter().cloned())
        .collect();
    let turns = detail
        .questions
        .iter()
        .filter(|question| selected.is_empty() || selected.contains(&question.question.id))
        .flat_map(|question| question.turns.iter().cloned())
        .collect();
    let parts = detail
        .questions
        .iter()
        .filter(|question| selected.is_empty() || selected.contains(&question.question.id))
        .flat_map(|question| question.parts.iter().cloned())
        .collect();
    serde_json::to_string_pretty(&ConversationRawExport {
        schema_version: 1,
        format: "raw",
        record_kind,
        source,
        session: &detail.session,
        questions,
        question_turns,
        turns,
        parts,
    })
    .map_err(AppError::external)
}

pub(crate) fn export_question_title(
    question: &crate::backend::domain::conversations::ConversationQuestionDetail,
) -> String {
    question
        .question
        .title
        .as_deref()
        .filter(|title| !title.trim().is_empty())
        .map(str::to_string)
        .or_else(|| {
            question
                .turns
                .iter()
                .map(|turn| turn.user_text.trim())
                .find(|text| !text.is_empty())
                .map(|text| text.chars().take(96).collect())
        })
        .unwrap_or_else(|| "Question".to_string())
}

pub(crate) fn humanize_card_kind(kind: &str) -> String {
    kind.rsplit('.')
        .next()
        .unwrap_or(kind)
        .replace(['-', '_'], " ")
}

pub(crate) fn validate_export_question_ids(
    detail: &crate::backend::domain::conversations::ConversationSessionDetail,
    question_ids: &[String],
) -> AppResult<()> {
    if question_ids.is_empty() {
        return Ok(());
    }
    let available = detail
        .questions
        .iter()
        .map(|question| &question.question.id)
        .collect::<BTreeSet<_>>();
    if let Some(missing_id) = question_ids
        .iter()
        .find(|question_id| !available.contains(question_id))
    {
        return Err(AppError::NotFound(format!(
            "conversation question not found in session: {missing_id}"
        )));
    }
    Ok(())
}

pub(crate) fn default_export_relative_path(
    detail: &crate::backend::domain::conversations::ConversationSessionDetail,
    question_ids: &[String],
    fallback_project_segment: &str,
    format: ConversationExportFormat,
) -> PathBuf {
    let project_segment = detail
        .session
        .project_path
        .as_deref()
        .and_then(|path| Path::new(path).file_name())
        .and_then(|name| name.to_str())
        .unwrap_or(fallback_project_segment);
    let short_id = detail
        .session
        .id
        .chars()
        .rev()
        .take(8)
        .collect::<String>()
        .chars()
        .rev()
        .collect::<String>();
    let question_count = question_ids.len();
    let file_stem = if question_count == 0 {
        sanitize_path_segment(&detail.session.title)
    } else {
        format!(
            "{}-selected-{question_count}",
            sanitize_path_segment(&detail.session.title)
        )
    };
    let extension = match format {
        ConversationExportFormat::Rendered => "md",
        ConversationExportFormat::Raw => "json",
    };
    PathBuf::from(sanitize_path_segment(&detail.session.adapter_id))
        .join(sanitize_path_segment(project_segment))
        .join(format!("{file_stem}-{short_id}.{extension}"))
}

pub(crate) fn export_format_label(format: ConversationExportFormat) -> &'static str {
    match format {
        ConversationExportFormat::Rendered => "rendered",
        ConversationExportFormat::Raw => "raw",
    }
}

pub(crate) fn relative_path_text(path: &Path) -> String {
    path.components()
        .filter_map(|component| match component {
            std::path::Component::Normal(segment) => Some(segment.to_string_lossy().to_string()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

pub(crate) fn validate_export_relative_path(value: &str) -> AppResult<PathBuf> {
    let value = value.trim();
    if value.is_empty() {
        return Err(AppError::Validation(
            "markdown_export relative_path is required".to_string(),
        ));
    }
    let path = Path::new(value);
    if path.is_absolute() {
        return Err(AppError::Validation(
            "markdown_export relative_path must be relative".to_string(),
        ));
    }
    let mut relative = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::Normal(segment) => relative.push(segment),
            _ => {
                return Err(AppError::Validation(
                    "markdown_export relative_path cannot contain root, prefix, '.', or '..'"
                        .to_string(),
                ))
            }
        }
    }
    if relative.as_os_str().is_empty() {
        return Err(AppError::Validation(
            "markdown_export relative_path is required".to_string(),
        ));
    }
    Ok(relative)
}

pub(crate) fn write_export_content(
    output_root: &Path,
    relative_path: &Path,
    content: &str,
) -> AppResult<()> {
    fs::create_dir_all(output_root)?;
    let relative_parent = relative_path.parent().unwrap_or_else(|| Path::new(""));
    let parent = prepare_export_parent(output_root, relative_parent)?;
    let target_path = output_root.join(relative_path);
    if let Ok(metadata) = fs::symlink_metadata(&target_path) {
        if metadata.file_type().is_symlink() {
            return Err(AppError::Conflict(format!(
                "markdown_export relative_path points to a symlink: {}",
                relative_path.display()
            )));
        }
        if metadata.is_dir() {
            return Err(AppError::Conflict(format!(
                "markdown_export relative_path points to a directory: {}",
                relative_path.display()
            )));
        }
    }
    ensure_export_parent_stays_in_root(output_root, &parent)?;
    fs::write(&target_path, content).map_err(AppError::from)
}

pub(crate) fn prepare_export_parent(
    output_root: &Path,
    relative_parent: &Path,
) -> AppResult<PathBuf> {
    let mut current = output_root.to_path_buf();
    for component in relative_parent.components() {
        let std::path::Component::Normal(segment) = component else {
            return Err(AppError::Validation(
                "markdown_export relative_path cannot contain root, prefix, '.', or '..'"
                    .to_string(),
            ));
        };
        current.push(segment);
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(AppError::Conflict(format!(
                    "markdown_export relative_path cannot traverse symlink: {}",
                    current.display()
                )));
            }
            Ok(metadata) if metadata.is_dir() => {}
            Ok(_) => {
                return Err(AppError::Conflict(format!(
                    "markdown_export relative_path parent is not a directory: {}",
                    current.display()
                )));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                fs::create_dir(&current)?;
            }
            Err(error) => return Err(AppError::from(error)),
        }
    }
    Ok(current)
}

pub(crate) fn ensure_export_parent_stays_in_root(
    output_root: &Path,
    parent: &Path,
) -> AppResult<()> {
    let canonical_root = output_root.canonicalize()?;
    let canonical_parent = parent.canonicalize()?;
    if !canonical_parent.starts_with(&canonical_root) {
        return Err(AppError::Conflict(
            "markdown_export relative_path resolves outside output_root".to_string(),
        ));
    }
    Ok(())
}

pub(crate) fn normalize_conversation_record_kind(
    record_kind: Option<&str>,
) -> AppResult<(
    String,
    crate::backend::domain::conversations::ConversationRecordKind,
)> {
    let record_kind = record_kind
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("session");
    match record_kind {
        "session" | "sessions" => Ok((
            "session".to_string(),
            crate::backend::domain::conversations::ConversationRecordKind::Session,
        )),
        "web" | "web-record" | "web_record" | "web-records" | "web_records" => Ok((
            "web".to_string(),
            crate::backend::domain::conversations::ConversationRecordKind::Web,
        )),
        other => Err(AppError::Validation(format!(
            "unsupported conversation record kind: {other}"
        ))),
    }
}

pub(crate) fn conversation_record_kind_from_locator(
    locator: &str,
) -> AppResult<crate::backend::domain::conversations::ConversationRecordKind> {
    let locator = locator.trim();
    if locator.starts_with("web-record-question-")
        || locator.starts_with("web-record-turn-")
        || locator.starts_with("web-record-part-")
    {
        return Ok(crate::backend::domain::conversations::ConversationRecordKind::Web);
    }
    if locator.starts_with("conversation-question-")
        || locator.starts_with("conversation-turn-")
        || locator.starts_with("conversation-part-")
    {
        return Ok(crate::backend::domain::conversations::ConversationRecordKind::Session);
    }
    Err(AppError::Validation(format!(
        "conversation locator must use a full conversation-* or web-record-* identifier: {locator}"
    )))
}

pub(crate) fn sanitize_path_segment(value: &str) -> String {
    let mut segment = String::new();
    let mut last_was_separator = false;
    for character in value.trim().chars() {
        if character.is_alphanumeric() || matches!(character, '_' | '.') {
            segment.push(character);
            last_was_separator = false;
        } else if !last_was_separator && !segment.is_empty() {
            segment.push('-');
            last_was_separator = true;
        }
        if segment.chars().count() >= 96 {
            break;
        }
    }
    let segment = segment.trim_matches(['-', '.']).to_string();
    if segment.is_empty() {
        "untitled".to_string()
    } else {
        segment
    }
}
