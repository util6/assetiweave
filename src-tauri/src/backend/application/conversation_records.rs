use super::prelude::*;

impl AppService {
    pub(crate) fn list_conversation_sessions(
        &self,
        params: ConversationSessionListParams,
    ) -> AppResult<Vec<crate::backend::dto::ConversationSessionListItem>> {
        let pool = self.db.pool().clone();
        let adapter_id = params.adapter_id;
        let source_id = params.source_id;
        let query = params.query;
        let limit = params.limit.unwrap_or(50).clamp(1, 500);
        let offset = params.offset.unwrap_or(0);
        self.db.block_on(async move {
            crate::backend::store::list_conversation_sessions_sqlx(
                &pool,
                adapter_id.as_deref(),
                source_id.as_deref(),
                query.as_deref(),
                limit,
                offset,
            )
            .await
        })
    }

    pub(crate) fn get_conversation_session(
        &self,
        params: ConversationSessionGetParams,
    ) -> AppResult<crate::backend::dto::ConversationSessionDetail> {
        let pool = self.db.pool().clone();
        self.db.block_on(async move {
            crate::backend::store::load_conversation_session_detail_sqlx(&pool, &params.session_id)
                .await
        })
    }

    pub(crate) fn list_web_record_sessions(
        &self,
        params: ConversationSessionListParams,
    ) -> AppResult<Vec<crate::backend::dto::ConversationSessionListItem>> {
        let pool = self.db.pool().clone();
        let adapter_id = params.adapter_id;
        let source_id = params.source_id;
        let query = params.query;
        let limit = params.limit.unwrap_or(50).clamp(1, 500);
        let offset = params.offset.unwrap_or(0);
        self.db.block_on(async move {
            crate::backend::store::list_web_record_sessions_sqlx(
                &pool,
                adapter_id.as_deref(),
                source_id.as_deref(),
                query.as_deref(),
                limit,
                offset,
            )
            .await
        })
    }

    pub(crate) fn get_web_record_session(
        &self,
        params: ConversationSessionGetParams,
    ) -> AppResult<crate::backend::dto::ConversationSessionDetail> {
        let pool = self.db.pool().clone();
        self.db.block_on(async move {
            crate::backend::store::load_web_record_session_detail_sqlx(&pool, &params.session_id)
                .await
        })
    }

    pub(crate) fn search_conversation_records(
        &self,
        params: ConversationSearchParams,
    ) -> AppResult<ConversationSearchResult> {
        let query = params.query.trim();
        if query.is_empty() {
            return Err("conversation search query is required".to_string());
        }
        let (record_kind_label, record_kind) =
            normalize_conversation_record_kind(params.record_kind.as_deref())?;
        let limit = params.limit.unwrap_or(50).clamp(1, 500);
        let offset = params.offset.unwrap_or(0);
        let pool = self.db.pool().clone();
        let adapter_id = params.adapter_id.clone();
        let source_id = params.source_id.clone();
        let project_path = params.project_path.clone();
        let query = query.to_string();
        let search_query = query.clone();
        let content_types = params.content_types.clone();
        let since = params.since.clone();
        let until = params.until.clone();
        let timeline = params.timeline;
        let page = self.db.block_on(async move {
            crate::backend::store::search_conversation_cards_sqlx(
                &pool,
                record_kind,
                adapter_id.as_deref(),
                source_id.as_deref(),
                project_path.as_deref(),
                &search_query,
                &content_types,
                since.as_deref(),
                until.as_deref(),
                timeline,
                limit,
                offset,
            )
            .await
        })?;
        Ok(ConversationSearchResult {
            query: query.to_string(),
            record_kind: record_kind_label.clone(),
            scope: ConversationSearchScope {
                record_kind: record_kind_label,
                adapter_id: params.adapter_id,
                source_id: params.source_id,
                project_path: params.project_path,
                query: query.to_string(),
                content_types: params.content_types,
                since: params.since,
                until: params.until,
                timeline: params.timeline,
                limit,
                offset,
            },
            total_count: page.total_count,
            hits: page.hits,
        })
    }

    pub(crate) fn export_conversation_session(
        &self,
        params: ConversationSessionExportParams,
    ) -> AppResult<Value> {
        let pool = self.db.pool().clone();
        let session_id = params.session_id.clone();
        let detail = self.db.block_on(async move {
            crate::backend::store::load_conversation_session_detail_sqlx(&pool, &session_id).await
        })?;
        let output_root = crate::backend::path_utils::expand_path(&params.output_root)?;
        let project_segment = detail
            .session
            .project_path
            .as_deref()
            .and_then(|path| Path::new(path).file_name())
            .and_then(|name| name.to_str())
            .unwrap_or("unknown-project");
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
        let question_count = params.question_ids.len();
        let file_stem = if question_count == 0 {
            sanitize_path_segment(&detail.session.title)
        } else {
            format!(
                "{}-selected-{question_count}",
                sanitize_path_segment(&detail.session.title)
            )
        };
        let target_path = output_root
            .join(sanitize_path_segment(&detail.session.adapter_id))
            .join(sanitize_path_segment(project_segment))
            .join(format!("{file_stem}-{short_id}.md"));
        let content = crate::backend::store::render_conversation_detail_markdown_with_filter(
            &detail,
            &params.question_ids,
            &params.content_filter,
        )?;
        if params.dry_run {
            return Ok(json!({
                "dry_run": true,
                "written": false,
                "path": target_path,
                "bytes": content.len(),
                "question_ids": params.question_ids,
                "question_count": question_count
            }));
        }
        if let Some(parent) = target_path.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        fs::write(&target_path, &content).map_err(|error| error.to_string())?;
        Ok(json!({
            "dry_run": false,
            "written": true,
            "path": target_path,
            "bytes": content.len(),
            "question_ids": params.question_ids,
            "question_count": question_count
        }))
    }

    pub(crate) fn export_web_record_session(
        &self,
        params: ConversationSessionExportParams,
    ) -> AppResult<Value> {
        let pool = self.db.pool().clone();
        let session_id = params.session_id.clone();
        let detail = self.db.block_on(async move {
            crate::backend::store::load_web_record_session_detail_sqlx(&pool, &session_id).await
        })?;
        let output_root = crate::backend::path_utils::expand_path(&params.output_root)?;
        let project_segment = detail
            .session
            .project_path
            .as_deref()
            .and_then(|path| Path::new(path).file_name())
            .and_then(|name| name.to_str())
            .unwrap_or("web");
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
        let question_count = params.question_ids.len();
        let file_stem = if question_count == 0 {
            sanitize_path_segment(&detail.session.title)
        } else {
            format!(
                "{}-selected-{question_count}",
                sanitize_path_segment(&detail.session.title)
            )
        };
        let target_path = output_root
            .join(sanitize_path_segment(&detail.session.adapter_id))
            .join(sanitize_path_segment(project_segment))
            .join(format!("{file_stem}-{short_id}.md"));
        let content = crate::backend::store::render_web_record_detail_markdown_with_filter(
            &detail,
            &params.question_ids,
            &params.content_filter,
        )?;
        if params.dry_run {
            return Ok(json!({
                "dry_run": true,
                "written": false,
                "path": target_path,
                "bytes": content.len(),
                "question_ids": params.question_ids,
                "question_count": question_count
            }));
        }
        if let Some(parent) = target_path.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        fs::write(&target_path, &content).map_err(|error| error.to_string())?;
        Ok(json!({
            "dry_run": false,
            "written": true,
            "path": target_path,
            "bytes": content.len(),
            "question_ids": params.question_ids,
            "question_count": question_count
        }))
    }

    pub(crate) fn list_conversation_questions(
        &self,
        params: ConversationQuestionListParams,
    ) -> AppResult<Vec<crate::backend::dto::ConversationQuestionDetail>> {
        let pool = self.db.pool().clone();
        let session_id = params.session_id;
        let query = params.query;
        let limit = params.limit.unwrap_or(100).clamp(1, 500);
        let offset = params.offset.unwrap_or(0);
        self.db.block_on(async move {
            crate::backend::store::list_conversation_question_details_sqlx(
                &pool,
                &session_id,
                query.as_deref(),
                limit,
                offset,
            )
            .await
        })
    }

    pub(crate) fn get_conversation_question(
        &self,
        params: ConversationQuestionGetParams,
    ) -> AppResult<crate::backend::dto::ConversationQuestionDetail> {
        let pool = self.db.pool().clone();
        self.db.block_on(async move {
            crate::backend::store::load_conversation_question_detail_sqlx(
                &pool,
                &params.question_id,
            )
            .await
        })
    }

    pub(crate) fn merge_conversation_questions(
        &self,
        params: ConversationQuestionMergeParams,
    ) -> AppResult<crate::backend::dto::ConversationMutationResult> {
        let pool = self.db.pool().clone();
        self.db.block_on(async move {
            crate::backend::store::merge_conversation_questions_sqlx(
                &pool,
                &params.question_ids,
                params.dry_run,
            )
            .await
        })
    }

    pub(crate) fn split_conversation_question(
        &self,
        params: ConversationQuestionSplitParams,
    ) -> AppResult<crate::backend::dto::ConversationMutationResult> {
        let pool = self.db.pool().clone();
        self.db.block_on(async move {
            crate::backend::store::split_conversation_question_sqlx(
                &pool,
                &params.question_id,
                &params.before_turn_id,
                params.dry_run,
            )
            .await
        })
    }
}

fn normalize_conversation_record_kind(
    record_kind: Option<&str>,
) -> AppResult<(String, crate::backend::dto::ConversationRecordKind)> {
    let record_kind = record_kind
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("session");
    match record_kind {
        "session" | "sessions" => Ok((
            "session".to_string(),
            crate::backend::dto::ConversationRecordKind::Session,
        )),
        "web" | "web-record" | "web_record" | "web-records" | "web_records" => Ok((
            "web".to_string(),
            crate::backend::dto::ConversationRecordKind::Web,
        )),
        other => Err(format!("unsupported conversation record kind: {other}")),
    }
}

fn sanitize_path_segment(value: &str) -> String {
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
