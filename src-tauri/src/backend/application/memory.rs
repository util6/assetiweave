use super::prelude::*;
use crate::backend::runtime::{AppError, AppResult};

impl AppService {
    pub(crate) fn list_memory_items(
        &self,
        params: MemoryItemListParams,
    ) -> AppResult<MemoryItemPage> {
        validate_filter_count("kind", params.kinds.len())?;
        validate_filter_count("status", params.statuses.len())?;
        validate_filter_count("origin", params.origins.len())?;
        let limit = params.limit.unwrap_or(50).clamp(1, 200);
        let offset = params.offset.unwrap_or(0);
        let scope_fingerprint = params
            .scope
            .as_ref()
            .map(MemoryScope::fingerprint)
            .transpose()
            .map_err(AppError::external)?;
        let filter = MemoryItemFilter {
            kinds: params.kinds,
            statuses: params.statuses,
            origins: params.origins,
            scope_fingerprint,
            stale_only: params.stale_only,
            limit,
            offset,
        };
        let pool = self.db.pool().clone();
        let tenant_id = self.tenant_id().to_string();
        Ok(self.db.block_on(async move {
            let total_count =
                crate::backend::store::count_memory_items_sqlx(&pool, &tenant_id, &filter)
                    .await
                    .map_err(AppError::external)?;
            let items = crate::backend::store::list_memory_items_sqlx(&pool, &tenant_id, &filter)
                .await
                .map_err(AppError::external)?;
            Ok::<MemoryItemPage, AppError>(MemoryItemPage {
                total_count,
                items,
                limit,
                offset,
            })
        })?)
    }

    pub(crate) fn get_memory_item(
        &self,
        params: MemoryItemGetParams,
    ) -> AppResult<MemoryItemDetail> {
        validate_memory_item_id(&params.item_id)?;
        let pool = self.db.pool().clone();
        let tenant_id = self.tenant_id().to_string();
        let item_id = params.item_id;
        Ok(self.db.block_on(async move {
            crate::backend::store::load_memory_item_detail_sqlx(&pool, &tenant_id, &item_id)
                .await
                .map_err(AppError::external)?
                .ok_or_else(|| AppError::NotFound(format!("memory item {item_id} was not found")))
        })?)
    }

    pub(crate) fn create_memory_item(
        &self,
        params: MemoryItemCreateParams,
    ) -> AppResult<MemoryItemDetail> {
        validate_memory_content(&params.title, &params.content_markdown)?;
        validate_memory_scope(&params.scope)?;
        validate_evidence_ids(&params.evidence_ids)?;
        let draft = NewMemoryItem {
            kind: params.kind,
            status: MemoryItemStatus::Active,
            title: params.title,
            content_markdown: params.content_markdown,
            scope: params.scope,
            origin: MemoryItemOrigin::Manual,
            origin_run_id: None,
            origin_dream_note_id: None,
            origin_extraction_id: None,
            confidence: params.confidence,
            supersedes_item_id: None,
            source_revision: 0,
            verified_revision: 0,
            stale_reason: None,
        };
        let pool = self.db.pool().clone();
        let tenant_id = self.tenant_id().to_string();
        Ok(self
            .db
            .block_on(async move {
                crate::backend::store::create_memory_item_sqlx(
                    &pool,
                    &tenant_id,
                    &draft,
                    &params.evidence_ids,
                )
                .await
            })
            .map_err(AppError::external)?)
    }

    pub(crate) fn update_memory_item(
        &self,
        params: MemoryItemUpdateParams,
    ) -> AppResult<MemoryItemDetail> {
        let mut detail = self.get_memory_item(MemoryItemGetParams {
            item_id: params.item_id,
        })?;
        if matches!(
            detail.item.status,
            MemoryItemStatus::Archived | MemoryItemStatus::Rejected | MemoryItemStatus::Superseded
        ) {
            return Err(AppError::Conflict(format!(
                "memory item {} cannot be edited while status is {}",
                detail.item.id,
                memory_status_label(detail.item.status)
            )));
        }
        apply_memory_item_changes(
            &mut detail.item,
            params.kind,
            params.title,
            params.content_markdown,
            params.scope,
            params.confidence,
        )?;
        if let Some(evidence_ids) = params.evidence_ids.as_deref() {
            validate_evidence_ids(evidence_ids)?;
        }
        self.persist_memory_item_update(
            detail.item,
            params.evidence_ids.as_deref(),
            MemoryRevisionChangeKind::Update,
        )
    }

    pub(crate) fn archive_memory_item(
        &self,
        params: MemoryItemGetParams,
    ) -> AppResult<MemoryItemDetail> {
        let mut detail = self.get_memory_item(params)?;
        if detail.item.status == MemoryItemStatus::Archived {
            return Ok(detail);
        }
        if detail.item.status == MemoryItemStatus::Rejected {
            return Err(AppError::Conflict(
                "rejected memory candidates cannot be archived".to_string(),
            ));
        }
        detail.item.status = MemoryItemStatus::Archived;
        self.persist_memory_item_update(detail.item, None, MemoryRevisionChangeKind::Status)
    }

    pub(crate) fn accept_memory_candidate(
        &self,
        params: MemoryCandidateAcceptParams,
    ) -> AppResult<MemoryItemDetail> {
        let mut detail = self.get_memory_item(MemoryItemGetParams {
            item_id: params.item_id,
        })?;
        if detail.item.status == MemoryItemStatus::Active {
            return Ok(detail);
        }
        if detail.item.status != MemoryItemStatus::Candidate {
            return Err(AppError::Validation(format!(
                "memory item {} is not a reviewable candidate",
                detail.item.id
            )));
        }
        apply_memory_item_changes(
            &mut detail.item,
            params.kind,
            params.title,
            params.content_markdown,
            params.scope,
            params.confidence,
        )?;
        detail.item.status = MemoryItemStatus::Active;
        let evidence_ids = params.evidence_ids.unwrap_or_else(|| {
            detail
                .evidence
                .iter()
                .map(|evidence| evidence.id.clone())
                .collect()
        });
        validate_evidence_ids(&evidence_ids)?;
        self.persist_memory_item_update(
            detail.item,
            Some(&evidence_ids),
            MemoryRevisionChangeKind::Accept,
        )
    }

    pub(crate) fn reject_memory_candidate(
        &self,
        params: MemoryItemGetParams,
    ) -> AppResult<MemoryItemDetail> {
        let mut detail = self.get_memory_item(params)?;
        if detail.item.status == MemoryItemStatus::Rejected {
            return Ok(detail);
        }
        if detail.item.status != MemoryItemStatus::Candidate {
            return Err(AppError::Validation(format!(
                "memory item {} is not a reviewable candidate",
                detail.item.id
            )));
        }
        detail.item.status = MemoryItemStatus::Rejected;
        self.persist_memory_item_update(detail.item, None, MemoryRevisionChangeKind::Status)
    }

    pub(crate) fn verify_memory(
        &self,
        params: MemoryVerifyParams,
    ) -> AppResult<MemoryVerifyResult> {
        if params.item_ids.is_empty() {
            return Err(AppError::Validation(
                "memory.verify requires at least one item id".to_string(),
            ));
        }
        if params.item_ids.len() > 200 {
            return Err(AppError::Validation(
                "memory.verify accepts at most 200 item ids".to_string(),
            ));
        }
        let mut unique_ids = Vec::new();
        let mut seen = HashSet::new();
        for item_id in params.item_ids {
            validate_memory_item_id(&item_id)?;
            if seen.insert(item_id.clone()) {
                unique_ids.push(item_id);
            }
        }
        let source_revision = self
            .db
            .block_on(crate::backend::store::load_memory_source_revision_sqlx(
                self.db.pool(),
                self.tenant_id(),
            ))
            .map_err(AppError::external)?;
        let mut unchanged_revision = true;
        let mut results = Vec::with_capacity(unique_ids.len());
        for item_id in unique_ids {
            let mut detail = self.get_memory_item(MemoryItemGetParams { item_id })?;
            if detail.item.source_revision == source_revision {
                results.push(detail);
                continue;
            }
            unchanged_revision = false;
            let mut reason = None;
            for evidence in &detail.evidence {
                let evidence_reason = self
                    .db
                    .block_on(crate::backend::store::memory_evidence_stale_reason_sqlx(
                        self.db.pool(),
                        self.tenant_id(),
                        evidence,
                    ))
                    .map_err(AppError::external)?;
                reason = stronger_stale_reason(reason, evidence_reason);
            }
            detail.item.source_revision = source_revision;
            detail.item.stale_reason = reason;
            if reason.is_none() {
                detail.item.verified_revision = source_revision;
            }
            results.push(self.persist_memory_item_update(
                detail.item,
                None,
                MemoryRevisionChangeKind::Status,
            )?);
        }
        Ok(MemoryVerifyResult {
            source_revision,
            unchanged_revision,
            items: results,
        })
    }

    fn persist_memory_item_update(
        &self,
        item: MemoryItem,
        evidence_ids: Option<&[String]>,
        change_kind: MemoryRevisionChangeKind,
    ) -> AppResult<MemoryItemDetail> {
        let pool = self.db.pool().clone();
        let tenant_id = self.tenant_id().to_string();
        let evidence_ids = evidence_ids.map(<[String]>::to_vec);
        Ok(self
            .db
            .block_on(async move {
                crate::backend::store::update_memory_item_sqlx(
                    &pool,
                    &tenant_id,
                    &item,
                    evidence_ids.as_deref(),
                    change_kind,
                )
                .await
            })
            .map_err(AppError::external)?)
    }
}

fn stronger_stale_reason(
    current: Option<MemoryStaleReason>,
    candidate: Option<MemoryStaleReason>,
) -> Option<MemoryStaleReason> {
    fn weight(reason: MemoryStaleReason) -> u8 {
        match reason {
            MemoryStaleReason::EvidenceChanged => 1,
            MemoryStaleReason::EvidenceMissing => 2,
            MemoryStaleReason::SourceUnavailable => 3,
        }
    }
    match (current, candidate) {
        (None, value) | (value, None) => value,
        (Some(left), Some(right)) => Some(if weight(right) > weight(left) {
            right
        } else {
            left
        }),
    }
}

fn apply_memory_item_changes(
    item: &mut MemoryItem,
    kind: Option<MemoryItemKind>,
    title: Option<String>,
    content_markdown: Option<String>,
    scope: Option<MemoryScope>,
    confidence: Option<f64>,
) -> AppResult<()> {
    if let Some(kind) = kind {
        item.kind = kind;
    }
    if let Some(title) = title {
        item.title = title;
    }
    if let Some(content_markdown) = content_markdown {
        item.content_markdown = content_markdown;
    }
    if let Some(scope) = scope {
        item.scope = scope;
    }
    if confidence.is_some() {
        item.confidence = confidence;
    }
    validate_memory_content(&item.title, &item.content_markdown)?;
    validate_memory_scope(&item.scope)
}

fn validate_memory_content(title: &str, content_markdown: &str) -> AppResult<()> {
    if title.trim().is_empty() {
        return Err(AppError::Validation(
            "memory item title is required".to_string(),
        ));
    }
    if title.chars().count() > 240 {
        return Err(AppError::Validation(
            "memory item title must not exceed 240 characters".to_string(),
        ));
    }
    if content_markdown.trim().is_empty() {
        return Err(AppError::Validation(
            "memory item content is required".to_string(),
        ));
    }
    if content_markdown.chars().count() > 65_536 {
        return Err(AppError::Validation(
            "memory item content must not exceed 65536 characters".to_string(),
        ));
    }
    Ok(())
}

pub(super) fn validate_memory_scope(scope: &MemoryScope) -> AppResult<()> {
    for (name, value, limit) in [
        ("app_id", scope.app_id.as_deref(), 256usize),
        ("source_id", scope.source_id.as_deref(), 256usize),
        ("project_path", scope.project_path.as_deref(), 4096usize),
        ("session_id", scope.session_id.as_deref(), 512usize),
    ] {
        if value.is_some_and(|value| value.chars().count() > limit) {
            return Err(AppError::Validation(format!(
                "memory scope {name} exceeds {limit} characters"
            )));
        }
    }
    Ok(())
}

fn validate_memory_item_id(item_id: &str) -> AppResult<()> {
    if item_id.trim().is_empty() {
        return Err(AppError::Validation(
            "memory item id is required".to_string(),
        ));
    }
    if item_id.chars().count() > 128 {
        return Err(AppError::Validation(
            "memory item id must not exceed 128 characters".to_string(),
        ));
    }
    Ok(())
}

fn validate_evidence_ids(evidence_ids: &[String]) -> AppResult<()> {
    if evidence_ids.len() > 256 {
        return Err(AppError::Validation(
            "memory item cannot reference more than 256 evidence snapshots".to_string(),
        ));
    }
    if evidence_ids
        .iter()
        .any(|id| id.trim().is_empty() || id.chars().count() > 128)
    {
        return Err(AppError::Validation(
            "memory evidence ids must be non-empty and at most 128 characters".to_string(),
        ));
    }
    Ok(())
}

fn validate_filter_count(name: &str, count: usize) -> AppResult<()> {
    if count > 16 {
        Err(AppError::Validation(format!(
            "memory item {name} filter accepts at most 16 values"
        )))
    } else {
        Ok(())
    }
}

fn memory_status_label(status: MemoryItemStatus) -> &'static str {
    match status {
        MemoryItemStatus::Candidate => "candidate",
        MemoryItemStatus::Active => "active",
        MemoryItemStatus::Completed => "completed",
        MemoryItemStatus::Superseded => "superseded",
        MemoryItemStatus::Archived => "archived",
        MemoryItemStatus::Rejected => "rejected",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::models::{MemoryEvidenceRecordKind, NewMemoryEvidenceSnapshot};

    #[test]
    fn memory_freshness_uses_specific_reason_precedence() {
        assert_eq!(
            stronger_stale_reason(
                Some(MemoryStaleReason::EvidenceChanged),
                Some(MemoryStaleReason::EvidenceMissing)
            ),
            Some(MemoryStaleReason::EvidenceMissing)
        );
        assert_eq!(
            stronger_stale_reason(
                Some(MemoryStaleReason::EvidenceMissing),
                Some(MemoryStaleReason::SourceUnavailable)
            ),
            Some(MemoryStaleReason::SourceUnavailable)
        );
    }

    #[test]
    fn memory_validation_errors_keep_runtime_error_category() {
        let error = validate_memory_content("", "content").expect_err("empty title rejected");

        assert_eq!(error.code(), "validation_error");
    }

    #[test]
    fn memory_item_app_service_supports_create_list_update_get_and_archive() {
        let (service, db_path) = test_service("crud");
        let created = service
            .create_memory_item(MemoryItemCreateParams {
                kind: MemoryItemKind::Decision,
                title: "One workflow boundary".to_string(),
                content_markdown: "Use AppService for desktop and CLI.".to_string(),
                scope: test_scope(),
                confidence: Some(1.0),
                evidence_ids: Vec::new(),
            })
            .expect("create memory item");

        let page = service
            .list_memory_items(MemoryItemListParams {
                statuses: vec![MemoryItemStatus::Active],
                ..MemoryItemListParams::default()
            })
            .expect("list memory items");
        assert_eq!(page.total_count, 1);
        assert_eq!(page.items[0].id, created.item.id);

        let updated = service
            .update_memory_item(MemoryItemUpdateParams {
                item_id: created.item.id.clone(),
                kind: None,
                title: Some("Shared workflow boundary".to_string()),
                content_markdown: None,
                scope: None,
                confidence: None,
                evidence_ids: None,
            })
            .expect("update memory item");
        assert_eq!(updated.item.title, "Shared workflow boundary");
        assert_eq!(updated.revisions.len(), 2);

        let loaded = service
            .get_memory_item(MemoryItemGetParams {
                item_id: created.item.id.clone(),
            })
            .expect("get memory item");
        assert_eq!(loaded.item.title, "Shared workflow boundary");

        let archived = service
            .archive_memory_item(MemoryItemGetParams {
                item_id: created.item.id,
            })
            .expect("archive memory item");
        assert_eq!(archived.item.status, MemoryItemStatus::Archived);
        assert_eq!(archived.revisions.len(), 3);
        cleanup(service, &db_path);
    }

    #[test]
    fn memory_item_app_service_reviews_candidates_without_bypassing_revisions() {
        let (service, db_path) = test_service("candidate-review");
        let accepted_candidate = seed_candidate(&service, "Candidate A");
        let rejected_candidate = seed_candidate(&service, "Candidate B");

        let accepted = service
            .accept_memory_candidate(MemoryCandidateAcceptParams {
                item_id: accepted_candidate.item.id,
                kind: None,
                title: Some("Accepted decision".to_string()),
                content_markdown: None,
                scope: None,
                confidence: None,
                evidence_ids: None,
            })
            .expect("accept candidate");
        assert_eq!(accepted.item.status, MemoryItemStatus::Active);
        assert_eq!(accepted.item.title, "Accepted decision");
        assert_eq!(accepted.evidence.len(), 1);
        assert_eq!(accepted.revisions.len(), 2);
        assert_eq!(
            accepted.revisions[0].change_kind,
            MemoryRevisionChangeKind::Accept
        );

        let rejected = service
            .reject_memory_candidate(MemoryItemGetParams {
                item_id: rejected_candidate.item.id,
            })
            .expect("reject candidate");
        assert_eq!(rejected.item.status, MemoryItemStatus::Rejected);
        assert_eq!(rejected.revisions.len(), 2);
        cleanup(service, &db_path);
    }

    fn seed_candidate(service: &AppService, title: &str) -> MemoryItemDetail {
        let pool = service.db.pool().clone();
        let tenant_id = service.tenant_id().to_string();
        service
            .db
            .block_on(async move {
                let evidence = crate::backend::store::upsert_memory_evidence_snapshot_sqlx(
                    &pool,
                    &tenant_id,
                    &NewMemoryEvidenceSnapshot {
                        record_kind: MemoryEvidenceRecordKind::Session,
                        source_id: Some("codex".to_string()),
                        session_id: format!("session-{title}"),
                        question_id: Some(format!("question-{title}")),
                        turn_id: Some(format!("turn-{title}")),
                        part_id: Some(format!("part-{title}")),
                        block_id: format!("part-{title}"),
                        content_hash: format!("hash-{title}"),
                        excerpt: format!("Evidence for {title}"),
                        translated_excerpt: None,
                        event_time: None,
                        source_revision: 1,
                        source_unavailable: false,
                    },
                )
                .await?;
                crate::backend::store::create_memory_item_sqlx(
                    &pool,
                    &tenant_id,
                    &NewMemoryItem {
                        kind: MemoryItemKind::Decision,
                        status: MemoryItemStatus::Candidate,
                        title: title.to_string(),
                        content_markdown: format!("Content for {title}"),
                        scope: test_scope(),
                        origin: MemoryItemOrigin::DeepRecall,
                        origin_run_id: None,
                        origin_dream_note_id: None,
                        origin_extraction_id: None,
                        confidence: Some(0.8),
                        supersedes_item_id: None,
                        source_revision: 1,
                        verified_revision: 1,
                        stale_reason: None,
                    },
                    &[evidence.id],
                )
                .await
            })
            .expect("seed candidate")
    }

    fn test_scope() -> MemoryScope {
        MemoryScope {
            project_path: Some("~/assetiweave".to_string()),
            ..MemoryScope::default()
        }
    }

    fn test_service(label: &str) -> (AppService, PathBuf) {
        let db_path = std::env::temp_dir().join(format!(
            "assetiweave-memory-item-{label}-{}.sqlite",
            Uuid::new_v4()
        ));
        let db = crate::backend::store::Database::open(&db_path).expect("open database");
        let pool = db.pool().clone();
        let context = db
            .block_on(
                async move { crate::backend::store::load_local_request_context_sqlx(&pool).await },
            )
            .expect("load request context");
        let runtime_manager =
            std::sync::Arc::new(crate::backend::agent_market::AgentRuntimeManager::new(
                db.pool().clone(),
                db_path.with_extension("agent-executions"),
            ));
        let agent_runtime = runtime_manager.runtime();
        let app_runtime = crate::backend::runtime::AppRuntime::for_test(
            db_path.clone(),
            db.clone(),
            context.clone(),
            runtime_manager.clone(),
            agent_runtime.clone(),
        );
        let service = AppService {
            db,
            db_path: db_path.clone(),
            context,
            runtime: app_runtime.clone(),
            agent_runtime_manager: runtime_manager,
            agent_runtime,
            conversation_adapter_catalog: app_runtime.conversation_adapter_catalog(),
        };
        (service, db_path)
    }

    fn cleanup(service: AppService, db_path: &Path) {
        drop(service);
        let _ = std::fs::remove_file(db_path);
        let _ = std::fs::remove_file(db_path.with_extension("sqlite-wal"));
        let _ = std::fs::remove_file(db_path.with_extension("sqlite-shm"));
    }
}
