use super::prelude::*;

impl AppService {
    pub(crate) fn list_conversation_sessions(
        &self,
        params: ConversationSessionListParams,
    ) -> AppResult<Vec<crate::backend::dto::ConversationSessionListItem>> {
        let pool = self.db.pool().clone();
        let tenant_id = self.tenant_id().to_string();
        let adapter_id = params.adapter_id;
        let source_id = params.source_id;
        let query = params.query;
        let limit = params.limit.unwrap_or(50).clamp(1, 500);
        let offset = params.offset.unwrap_or(0);
        self.db.block_on(async move {
            crate::backend::store::list_conversation_sessions_sqlx(
                &pool,
                &tenant_id,
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
        let tenant_id = self.tenant_id().to_string();
        self.db.block_on(async move {
            crate::backend::store::load_conversation_session_detail_sqlx(
                &pool,
                &tenant_id,
                &params.session_id,
            )
            .await
        })
    }

    pub(crate) fn list_web_record_sessions(
        &self,
        params: ConversationSessionListParams,
    ) -> AppResult<Vec<crate::backend::dto::ConversationSessionListItem>> {
        let pool = self.db.pool().clone();
        let tenant_id = self.tenant_id().to_string();
        let adapter_id = params.adapter_id;
        let source_id = params.source_id;
        let query = params.query;
        let limit = params.limit.unwrap_or(50).clamp(1, 500);
        let offset = params.offset.unwrap_or(0);
        self.db.block_on(async move {
            crate::backend::store::list_web_record_sessions_sqlx(
                &pool,
                &tenant_id,
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
        let tenant_id = self.tenant_id().to_string();
        self.db.block_on(async move {
            crate::backend::store::load_web_record_session_detail_sqlx(
                &pool,
                &tenant_id,
                &params.session_id,
            )
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
        let tenant_id = self.tenant_id().to_string();
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
                &tenant_id,
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
        let tenant_id = self.tenant_id().to_string();
        let session_id = params.session_id.clone();
        let (detail, adapter, source) = self.db.block_on(async move {
            let detail = crate::backend::store::load_conversation_session_detail_sqlx(
                &pool,
                &tenant_id,
                &session_id,
            )
            .await?;
            let adapter = load_export_adapter_for_detail(&pool, &tenant_id, &detail).await?;
            let source = load_export_source_for_detail(&pool, &tenant_id, &detail).await?;
            AppResult::Ok((detail, adapter, source))
        })?;
        export_loaded_conversation_markdown(
            detail,
            adapter,
            source,
            params,
            "session",
            "unknown-project",
        )
    }

    pub(crate) fn export_web_record_session(
        &self,
        params: ConversationSessionExportParams,
    ) -> AppResult<Value> {
        let pool = self.db.pool().clone();
        let tenant_id = self.tenant_id().to_string();
        let session_id = params.session_id.clone();
        let (detail, adapter, source) = self.db.block_on(async move {
            let detail = crate::backend::store::load_web_record_session_detail_sqlx(
                &pool,
                &tenant_id,
                &session_id,
            )
            .await?;
            let adapter = load_export_adapter_for_detail(&pool, &tenant_id, &detail).await?;
            let source = load_export_source_for_detail(&pool, &tenant_id, &detail).await?;
            AppResult::Ok((detail, adapter, source))
        })?;
        export_loaded_conversation_markdown(detail, adapter, source, params, "web", "web")
    }

    pub(crate) fn list_conversation_questions(
        &self,
        params: ConversationQuestionListParams,
    ) -> AppResult<Vec<crate::backend::dto::ConversationQuestionDetail>> {
        let pool = self.db.pool().clone();
        let tenant_id = self.tenant_id().to_string();
        let session_id = params.session_id;
        let query = params.query;
        let limit = params.limit.unwrap_or(100).clamp(1, 500);
        let offset = params.offset.unwrap_or(0);
        self.db.block_on(async move {
            crate::backend::store::list_conversation_question_details_sqlx(
                &pool,
                &tenant_id,
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
        let tenant_id = self.tenant_id().to_string();
        self.db.block_on(async move {
            crate::backend::store::load_conversation_question_detail_sqlx(
                &pool,
                &tenant_id,
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
        let tenant_id = self.tenant_id().to_string();
        self.db.block_on(async move {
            crate::backend::store::merge_conversation_questions_sqlx(
                &pool,
                &tenant_id,
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
        let tenant_id = self.tenant_id().to_string();
        self.db.block_on(async move {
            crate::backend::store::split_conversation_question_sqlx(
                &pool,
                &tenant_id,
                &params.question_id,
                &params.before_turn_id,
                params.dry_run,
            )
            .await
        })
    }
}

async fn load_export_adapter_for_detail(
    pool: &sqlx::SqlitePool,
    tenant_id: &str,
    detail: &crate::backend::dto::ConversationSessionDetail,
) -> AppResult<ConversationAdapter> {
    crate::backend::store::load_conversation_adapter_sqlx(
        pool,
        tenant_id,
        &detail.session.adapter_id,
    )
    .await?
    .ok_or_else(|| {
        format!(
            "conversation adapter not found: {}",
            detail.session.adapter_id
        )
    })
}

async fn load_export_source_for_detail(
    pool: &sqlx::SqlitePool,
    tenant_id: &str,
    detail: &crate::backend::dto::ConversationSessionDetail,
) -> AppResult<ConversationSource> {
    crate::backend::store::load_conversation_source_sqlx(pool, tenant_id, &detail.session.source_id)
        .await?
        .ok_or_else(|| {
            format!(
                "conversation source not found: {}",
                detail.session.source_id
            )
        })
}

fn export_loaded_conversation_markdown(
    detail: crate::backend::dto::ConversationSessionDetail,
    adapter: ConversationAdapter,
    source: ConversationSource,
    params: ConversationSessionExportParams,
    record_kind: &str,
    fallback_project_segment: &str,
) -> AppResult<Value> {
    validate_export_question_ids(&detail, &params.question_ids)?;
    let output_root = crate::backend::path_utils::expand_path(&params.output_root)?;
    let default_relative_path =
        default_export_relative_path(&detail, &params.question_ids, fallback_project_segment);
    let default_relative_path_text = relative_path_text(&default_relative_path);
    let export = crate::backend::conversations::export_external_adapter_markdown(
        &adapter,
        &source,
        &detail,
        &params.question_ids,
        &params.content_filter,
        record_kind,
        &default_relative_path_text,
    )?;
    let relative_path = validate_export_relative_path(&export.relative_path)?;
    let target_path = output_root.join(&relative_path);
    let question_count = params.question_ids.len();
    if params.dry_run {
        return Ok(json!({
            "dry_run": true,
            "written": false,
            "path": target_path,
            "bytes": export.content.len(),
            "question_ids": params.question_ids,
            "question_count": question_count
        }));
    }
    write_export_content(&output_root, &relative_path, &export.content)?;
    Ok(json!({
        "dry_run": false,
        "written": true,
        "path": target_path,
        "bytes": export.content.len(),
        "question_ids": params.question_ids,
        "question_count": question_count
    }))
}

fn validate_export_question_ids(
    detail: &crate::backend::dto::ConversationSessionDetail,
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
        return Err(format!(
            "conversation question not found in session: {missing_id}"
        ));
    }
    Ok(())
}

fn default_export_relative_path(
    detail: &crate::backend::dto::ConversationSessionDetail,
    question_ids: &[String],
    fallback_project_segment: &str,
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
    PathBuf::from(sanitize_path_segment(&detail.session.adapter_id))
        .join(sanitize_path_segment(project_segment))
        .join(format!("{file_stem}-{short_id}.md"))
}

fn relative_path_text(path: &Path) -> String {
    path.components()
        .filter_map(|component| match component {
            std::path::Component::Normal(segment) => Some(segment.to_string_lossy().to_string()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

fn validate_export_relative_path(value: &str) -> AppResult<PathBuf> {
    let value = value.trim();
    if value.is_empty() {
        return Err("markdown_export relative_path is required".to_string());
    }
    let path = Path::new(value);
    if path.is_absolute() {
        return Err("markdown_export relative_path must be relative".to_string());
    }
    let mut relative = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::Normal(segment) => relative.push(segment),
            _ => {
                return Err(
                    "markdown_export relative_path cannot contain root, prefix, '.', or '..'"
                        .to_string(),
                )
            }
        }
    }
    if relative.as_os_str().is_empty() {
        return Err("markdown_export relative_path is required".to_string());
    }
    Ok(relative)
}

fn write_export_content(output_root: &Path, relative_path: &Path, content: &str) -> AppResult<()> {
    fs::create_dir_all(output_root).map_err(|error| error.to_string())?;
    let relative_parent = relative_path.parent().unwrap_or_else(|| Path::new(""));
    let parent = prepare_export_parent(output_root, relative_parent)?;
    let target_path = output_root.join(relative_path);
    if let Ok(metadata) = fs::symlink_metadata(&target_path) {
        if metadata.file_type().is_symlink() {
            return Err(format!(
                "markdown_export relative_path points to a symlink: {}",
                relative_path.display()
            ));
        }
        if metadata.is_dir() {
            return Err(format!(
                "markdown_export relative_path points to a directory: {}",
                relative_path.display()
            ));
        }
    }
    ensure_export_parent_stays_in_root(output_root, &parent)?;
    fs::write(&target_path, content).map_err(|error| error.to_string())
}

fn prepare_export_parent(output_root: &Path, relative_parent: &Path) -> AppResult<PathBuf> {
    let mut current = output_root.to_path_buf();
    for component in relative_parent.components() {
        let std::path::Component::Normal(segment) = component else {
            return Err(
                "markdown_export relative_path cannot contain root, prefix, '.', or '..'"
                    .to_string(),
            );
        };
        current.push(segment);
        match fs::symlink_metadata(&current) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(format!(
                    "markdown_export relative_path cannot traverse symlink: {}",
                    current.display()
                ));
            }
            Ok(metadata) if metadata.is_dir() => {}
            Ok(_) => {
                return Err(format!(
                    "markdown_export relative_path parent is not a directory: {}",
                    current.display()
                ));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                fs::create_dir(&current).map_err(|error| error.to_string())?;
            }
            Err(error) => return Err(error.to_string()),
        }
    }
    Ok(current)
}

fn ensure_export_parent_stays_in_root(output_root: &Path, parent: &Path) -> AppResult<()> {
    let canonical_root = output_root
        .canonicalize()
        .map_err(|error| error.to_string())?;
    let canonical_parent = parent.canonicalize().map_err(|error| error.to_string())?;
    if !canonical_parent.starts_with(&canonical_root) {
        return Err("markdown_export relative_path resolves outside output_root".to_string());
    }
    Ok(())
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
