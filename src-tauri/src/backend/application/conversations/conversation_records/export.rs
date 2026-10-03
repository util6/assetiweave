use super::*;
use crate::backend::application::prelude::*;
use crate::backend::application::AppService;
use crate::backend::domain::conversations::{
    ConversationAdapter, ConversationExportFormat, ConversationSource,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
impl AppService {
    pub(crate) async fn export_conversation_session(
        &self,
        params: ConversationSessionExportParams,
    ) -> AppResult<Value> {
        let pool = self.pool();
        let tenant_id = self.tenant_id();
        let session_id = crate::backend::store::resolve_conversation_session_id_prefix_sqlx(
            pool,
            tenant_id,
            &params.session_id,
        )
        .await
        .map_err(AppError::external)?;
        let detail = crate::backend::store::load_conversation_session_detail_sqlx(
            pool,
            tenant_id,
            &session_id,
        )
        .await
        .map_err(AppError::external)?;
        let adapter = load_export_adapter_for_detail(pool, tenant_id, &detail).await?;
        let source = load_export_source_for_detail(pool, tenant_id, &detail).await?;
        if matches!(params.format, ConversationExportFormat::Rendered) {
            self.ensure_conversation_adapter_package_runtime_ready(&adapter)
                .await?;
        }
        let settings = self.app_settings_value();
        export_loaded_conversation_markdown(
            detail,
            adapter,
            source,
            params,
            "session",
            "unknown-project",
            &settings,
        )
        .await
    }

    pub(crate) async fn export_web_record_session(
        &self,
        params: ConversationSessionExportParams,
    ) -> AppResult<Value> {
        let pool = self.pool();
        let tenant_id = self.tenant_id();
        let session_id = crate::backend::store::resolve_web_record_session_id_prefix_sqlx(
            pool,
            tenant_id,
            &params.session_id,
        )
        .await?;
        let detail = crate::backend::store::load_web_record_session_detail_sqlx(
            pool,
            tenant_id,
            &session_id,
        )
        .await?;
        let adapter = load_export_adapter_for_detail(pool, tenant_id, &detail).await?;
        let source = load_export_source_for_detail(pool, tenant_id, &detail).await?;
        if matches!(params.format, ConversationExportFormat::Rendered) {
            self.ensure_conversation_adapter_package_runtime_ready(&adapter)
                .await?;
        }
        let settings = self.app_settings_value();
        export_loaded_conversation_markdown(
            detail, adapter, source, params, "web", "web", &settings,
        )
        .await
    }
}

pub(crate) async fn load_export_adapter_for_detail(
    pool: &sqlx::SqlitePool,
    tenant_id: &str,
    detail: &crate::backend::domain::conversations::ConversationSessionDetail,
) -> AppResult<ConversationAdapter> {
    super::super::conversation_storage::load_adapter(pool, tenant_id, &detail.session.adapter_id)
        .await
        .map_err(AppError::external)?
        .ok_or_else(|| {
            AppError::NotFound(format!(
                "conversation adapter not found: {}",
                detail.session.adapter_id
            ))
        })
}

pub(crate) async fn load_export_source_for_detail(
    pool: &sqlx::SqlitePool,
    tenant_id: &str,
    detail: &crate::backend::domain::conversations::ConversationSessionDetail,
) -> AppResult<ConversationSource> {
    super::super::conversation_sources::load_source(pool, tenant_id, &detail.session.source_id)
        .await
        .map_err(AppError::external)?
        .ok_or_else(|| {
            AppError::NotFound(format!(
                "conversation source not found: {}",
                detail.session.source_id
            ))
        })
}

pub(crate) async fn export_loaded_conversation_markdown(
    detail: crate::backend::domain::conversations::ConversationSessionDetail,
    adapter: ConversationAdapter,
    source: ConversationSource,
    params: ConversationSessionExportParams,
    record_kind: &str,
    fallback_project_segment: &str,
    settings: &Value,
) -> AppResult<Value> {
    validate_export_question_ids(&detail, &params.question_ids)?;
    let output_root = crate::backend::infrastructure::path_utils::expand_path(&params.output_root)?;
    let default_relative_path = default_export_relative_path(
        &detail,
        &params.question_ids,
        fallback_project_segment,
        params.format,
    );
    let default_relative_path_text = relative_path_text(&default_relative_path);
    let use_legacy_adapter_exporter = matches!(params.format, ConversationExportFormat::Rendered)
        && adapter.card_contract_version != Some(1)
        && adapter
            .capabilities
            .iter()
            .any(|capability| capability == "export_markdown");
    let (content, relative_path_text) = match params.format {
        ConversationExportFormat::Raw => (
            export_conversation_raw_json(&detail, &source, &params.question_ids, record_kind)?,
            default_relative_path_text,
        ),
        ConversationExportFormat::Rendered if use_legacy_adapter_exporter => {
            let export =
                crate::backend::infrastructure::conversations::export_external_adapter_markdown_with_settings(
                    &adapter,
                    &source,
                    &detail,
                    &params.question_ids,
                    &params.content_filter,
                    record_kind,
                    &default_relative_path_text,
                    settings,
                )
                .await
                .map_err(AppError::external)?;
            (export.content, export.relative_path)
        }
        ConversationExportFormat::Rendered => (
            export_conversation_rendered_markdown(
                &detail,
                &params.question_ids,
                &params.content_filter,
            ),
            default_relative_path_text,
        ),
    };
    let relative_path = validate_export_relative_path(&relative_path_text)?;
    let target_path = output_root.join(&relative_path);
    let question_count = params.question_ids.len();
    if params.dry_run {
        record_conversation_export_observation(
            &adapter.id,
            record_kind,
            true,
            params.format,
            use_legacy_adapter_exporter,
        );
        return Ok(json!({
            "dry_run": true,
            "written": false,
            "path": target_path,
            "bytes": content.len(),
            "question_ids": params.question_ids,
            "question_count": question_count,
            "format": export_format_label(params.format),
            "legacy_adapter_exporter_used": use_legacy_adapter_exporter
        }));
    }
    write_export_content(&output_root, &relative_path, &content)?;
    record_conversation_export_observation(
        &adapter.id,
        record_kind,
        false,
        params.format,
        use_legacy_adapter_exporter,
    );
    Ok(json!({
        "dry_run": false,
        "written": true,
        "path": target_path,
        "bytes": content.len(),
        "question_ids": params.question_ids,
        "question_count": question_count,
        "format": export_format_label(params.format),
        "legacy_adapter_exporter_used": use_legacy_adapter_exporter
    }))
}

pub(crate) fn record_conversation_export_observation(
    adapter_id: &str,
    record_kind: &str,
    dry_run: bool,
    format: ConversationExportFormat,
    legacy_adapter_exporter_used: bool,
) {
    tracing::info!(
        action = "conversation.export",
        adapter_id = %adapter_id,
        record_kind = %record_kind,
        dry_run = %dry_run,
        format = %export_format_label(format),
        legacy_adapter_exporter_used = %legacy_adapter_exporter_used,
        "Conversation export completed"
    );
}

pub(crate) fn export_conversation_rendered_markdown(
    detail: &crate::backend::domain::conversations::ConversationSessionDetail,
    question_ids: &[String],
    content_filter: &crate::backend::domain::conversations::ConversationExportContentFilter,
) -> String {
    let selected = question_ids.iter().collect::<BTreeSet<_>>();
    let mut output = format!("# {}\n", detail.session.title.trim());
    for (question_index, question) in detail.questions.iter().enumerate() {
        if !selected.is_empty() && !selected.contains(&question.question.id) {
            continue;
        }
        let prompt = question
            .turns
            .iter()
            .map(|turn| turn.user_text.trim())
            .filter(|text| !text.is_empty())
            .collect::<Vec<_>>()
            .join("\n\n");
        output.push_str(&format!(
            "\n## {}. {}\n\n{}\n",
            question_index + 1,
            export_question_title(question),
            prompt,
        ));
        for node in &question.projected_content_nodes {
            if !content_filter.is_visible_node(&node.node_type, node.semantic_role.as_deref()) {
                continue;
            }
            output.push_str(&format!(
                "\n### {}\n\n",
                humanize_card_kind(&node.node_type)
            ));
            match node.renderer {
                crate::backend::domain::conversations::ConversationCardRenderer::Markdown => {
                    output.push_str(node.content.trim());
                    output.push('\n');
                }
                crate::backend::domain::conversations::ConversationCardRenderer::Code => {
                    output.push_str(&format!(
                        "```{}\n{}\n```\n",
                        node.language.as_deref().unwrap_or(""),
                        node.content.trim_end()
                    ));
                }
                crate::backend::domain::conversations::ConversationCardRenderer::Json => {
                    output.push_str(&format!("```json\n{}\n```\n", node.content.trim_end()));
                }
                crate::backend::domain::conversations::ConversationCardRenderer::Command => {
                    output.push_str(&format!("```sh\n{}\n```\n", node.content.trim_end()));
                }
                crate::backend::domain::conversations::ConversationCardRenderer::Plain
                | crate::backend::domain::conversations::ConversationCardRenderer::Path
                | crate::backend::domain::conversations::ConversationCardRenderer::TerminalOutput
                | crate::backend::domain::conversations::ConversationCardRenderer::Diff
                | crate::backend::domain::conversations::ConversationCardRenderer::CompactAction => {
                    output.push_str(&format!("```text\n{}\n```\n", node.content.trim_end()));
                }
            }
        }
    }
    output
}
