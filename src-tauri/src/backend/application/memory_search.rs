use super::prelude::*;
use crate::backend::models::ConversationPart;
use crate::backend::runtime::{AppError, AppResult};

const RECALL_SEARCH_MAX_CORPUS: usize = 512;
const RECALL_SEARCH_MAX_LIMIT: usize = 128;
const RECALL_LEXICAL_WEIGHT: u64 = 100;

impl AppService {
    pub(crate) async fn search_memory_recall(
        &self,
        params: MemoryRecallSearchParams,
    ) -> AppResult<MemoryRecallSearchResult> {
        validate_recall_search_params(&params)?;
        let query = params.query.trim().to_string();
        let limit = params.limit.unwrap_or(24).clamp(1, RECALL_SEARCH_MAX_LIMIT);
        let corpus = self.load_recall_search_corpus(&params).await?;
        let mut hits = BTreeMap::<String, MemoryRecallSearchHit>::new();
        let mut lexical_backends = BTreeSet::new();

        for (record_kind, label) in [
            (MemoryRecordKind::Session, "session"),
            (MemoryRecordKind::Web, "web"),
        ] {
            if record_kind == MemoryRecordKind::Web && params.scope.project_path.is_some() {
                continue;
            }
            let result = self
                .search_conversation_records(ConversationSearchParams {
                    record_kind: Some(label.to_string()),
                    adapter_id: params.scope.app_id.clone(),
                    source_id: params.scope.source_id.clone(),
                    project_path: params.scope.project_path.clone(),
                    query: query.clone(),
                    content_types: Vec::new(),
                    card_kinds: Vec::new(),
                    semantic_roles: Vec::new(),
                    include_questions: None,
                    include_cards: None,
                    since: params.since.clone(),
                    until: params.until.clone(),
                    timeline: false,
                    limit: Some(RECALL_SEARCH_MAX_LIMIT),
                    offset: Some(0),
                    search_options: None,
                })
                .await?;
            lexical_backends.insert(result.backend);
            for hit in result.hits {
                let reference = MemoryRecallQuestionRef {
                    record_kind,
                    source_id: hit.session.session.source_id.clone(),
                    session_id: hit.session.session.id.clone(),
                    session_title: hit.session.session.title.clone(),
                    project_path: hit.session.session.project_path.clone(),
                    question_id: hit.question_id.clone(),
                    question_index: hit.question_index,
                };
                let Some(document) = self.load_recall_search_document(reference).await? else {
                    continue;
                };
                let key = recall_locator_key(
                    record_kind,
                    &hit.session.session.id,
                    &hit.question_id,
                    hit.turn_id.as_deref(),
                    hit.part_id.as_deref(),
                    &hit.block_id,
                );
                if !document_matches_search_hints(&document, &params) {
                    continue;
                }
                merge_recall_search_hit(
                    &mut hits,
                    key,
                    MemoryRecallSearchHit {
                        record_kind,
                        source_id: hit.session.session.source_id,
                        session_id: hit.session.session.id,
                        session_title: hit.session.session.title,
                        project_path: hit.session.session.project_path,
                        question_id: hit.question_id,
                        question_index: hit.question_index,
                        turn_id: hit.turn_id,
                        part_id: hit.part_id,
                        block_id: hit.block_id,
                        card_type: hit.card_type.as_str().to_string(),
                        snippet: hit.snippet,
                        lexical_score: hit.score as u64,
                        semantic_score: 0,
                        score: (hit.score as u64).saturating_mul(RECALL_LEXICAL_WEIGHT),
                        sources: vec!["lexical".to_string()],
                    },
                );
            }
        }

        let semantic_documents = corpus
            .values()
            .map(
                |document| crate::backend::search::memory_semantic::SemanticDocument {
                    key: recall_question_key(&document.reference),
                    text: document.search_text.clone(),
                },
            )
            .collect::<Vec<_>>();
        let semantic_matches = crate::backend::search::memory_semantic::rank_documents(
            &query,
            &semantic_documents,
            RECALL_SEARCH_MAX_CORPUS,
        );
        for semantic_match in semantic_matches {
            let Some(document) = corpus.get(&semantic_match.key) else {
                continue;
            };
            let part_documents = document
                .parts
                .iter()
                .map(
                    |part| crate::backend::search::memory_semantic::SemanticDocument {
                        key: part.block_id.clone(),
                        text: part.content.clone(),
                    },
                )
                .collect::<Vec<_>>();
            let best_part =
                crate::backend::search::memory_semantic::rank_documents(&query, &part_documents, 1)
                    .into_iter()
                    .next()
                    .and_then(|matched| {
                        document
                            .parts
                            .iter()
                            .find(|part| part.block_id == matched.key)
                            .map(|part| (part, matched.score))
                    });
            let Some((part, part_score)) = best_part else {
                continue;
            };
            let key = recall_locator_key(
                document.reference.record_kind,
                &document.reference.session_id,
                &document.reference.question_id,
                part.turn_id.as_deref(),
                part.part_id.as_deref(),
                &part.block_id,
            );
            merge_recall_search_hit(
                &mut hits,
                key,
                MemoryRecallSearchHit {
                    record_kind: document.reference.record_kind,
                    source_id: document.reference.source_id.clone(),
                    session_id: document.reference.session_id.clone(),
                    session_title: document.reference.session_title.clone(),
                    project_path: document.reference.project_path.clone(),
                    question_id: document.reference.question_id.clone(),
                    question_index: document.reference.question_index,
                    turn_id: part.turn_id.clone(),
                    part_id: part.part_id.clone(),
                    block_id: part.block_id.clone(),
                    card_type: part.card_type.clone(),
                    snippet: leading_recall_snippet(&part.content),
                    lexical_score: 0,
                    semantic_score: part_score.max(semantic_match.score),
                    score: part_score.max(semantic_match.score),
                    sources: vec!["semantic".to_string()],
                },
            );
        }

        let mut hits = hits.into_values().collect::<Vec<_>>();
        hits.sort_by(|left, right| {
            right
                .score
                .cmp(&left.score)
                .then_with(|| left.record_kind.as_str().cmp(right.record_kind.as_str()))
                .then_with(|| left.session_id.cmp(&right.session_id))
                .then_with(|| left.question_id.cmp(&right.question_id))
                .then_with(|| left.block_id.cmp(&right.block_id))
        });
        let total_count = hits.len();
        let offset = params.offset.unwrap_or(0);
        hits = hits.into_iter().skip(offset).take(limit).collect();
        let backend = match lexical_backends.into_iter().collect::<Vec<_>>().as_slice() {
            [] => "deterministic_semantic".to_string(),
            backends => format!("hybrid({})+deterministic_semantic", backends.join("+")),
        };
        Ok(MemoryRecallSearchResult {
            query,
            backend,
            total_count,
            hits,
        })
    }

    async fn load_recall_search_corpus(
        &self,
        params: &MemoryRecallSearchParams,
    ) -> AppResult<BTreeMap<String, RecallSearchDocument>> {
        let (total, references) = crate::backend::store::list_memory_recall_question_refs_sqlx(
            self.db.pool(),
            self.tenant_id(),
            &params.scope,
            params.since.as_deref(),
            params.until.as_deref(),
            false,
            RECALL_SEARCH_MAX_CORPUS,
            0,
        )
        .await
        .map_err(AppError::external)?;
        let _ = total;
        let mut corpus = BTreeMap::new();
        for reference in references {
            if let Some(document) = self.load_recall_search_document(reference).await? {
                if !document_matches_search_hints(&document, params) {
                    continue;
                }
                corpus.insert(recall_question_key(&document.reference), document);
            }
        }
        Ok(corpus)
    }

    async fn load_recall_search_document(
        &self,
        reference: MemoryRecallQuestionRef,
    ) -> AppResult<Option<RecallSearchDocument>> {
        let available = match reference.record_kind {
            MemoryRecordKind::Session => {
                sqlx::query_scalar::<_, i64>(
                    "SELECT EXISTS(SELECT 1 FROM conversation_sessions s JOIN conversation_sources source ON source.tenant_id=s.tenant_id AND source.id=s.source_id WHERE s.tenant_id=?1 AND s.id=?2 AND s.missing=0 AND source.enabled=1 AND source.adapter_id <> 'assetiweave-memory-recall')",
                )
                .bind(self.tenant_id())
                .bind(&reference.session_id)
                .fetch_one(self.db.pool())
                .await
            }
            MemoryRecordKind::Web => {
                sqlx::query_scalar::<_, i64>(
                    "SELECT EXISTS(SELECT 1 FROM web_record_sessions s JOIN conversation_sources source ON source.tenant_id=s.tenant_id AND source.id=s.source_id WHERE s.tenant_id=?1 AND s.id=?2 AND s.missing=0 AND source.enabled=1 AND source.adapter_id <> 'assetiweave-memory-recall')",
                )
                .bind(self.tenant_id())
                .bind(&reference.session_id)
                .fetch_one(self.db.pool())
                .await
            }
        }
        .map_err(AppError::Db)?;
        if available != 1 {
            return Ok(None);
        }
        let detail = self.load_recall_question(&reference).await?;
        if detail.turns.iter().all(|turn| turn.missing) {
            return Ok(None);
        }
        let parts = recall_evidence_parts(&detail);
        let search_text = format!(
            "{}\n{}\n{}",
            reference.session_title,
            detail.question.title.as_deref().unwrap_or_default(),
            parts
                .iter()
                .map(|part| part.content.as_str())
                .collect::<Vec<_>>()
                .join("\n")
        );
        Ok(Some(RecallSearchDocument {
            reference,
            search_text,
            parts,
            source_parts: detail.parts,
        }))
    }

    async fn load_recall_question(
        &self,
        reference: &MemoryRecallQuestionRef,
    ) -> AppResult<crate::backend::dto::ConversationQuestionDetail> {
        match reference.record_kind {
            MemoryRecordKind::Session => {
                self.get_conversation_question(
                    crate::backend::application::ConversationQuestionGetParams {
                        question_id: reference.question_id.clone(),
                    },
                )
                .await
            }
            MemoryRecordKind::Web => self
                .get_web_record_session(crate::backend::application::ConversationSessionGetParams {
                    session_id: reference.session_id.clone(),
                })
                .await?
                .questions
                .into_iter()
                .find(|detail| detail.question.id == reference.question_id)
                .ok_or_else(|| {
                    AppError::NotFound(format!(
                        "web Recall question {} was not found",
                        reference.question_id
                    ))
                }),
        }
    }
}

#[derive(Debug, Clone)]
struct RecallPart {
    turn_id: Option<String>,
    part_id: Option<String>,
    block_id: String,
    card_type: String,
    content: String,
}

#[derive(Debug, Clone)]
struct RecallSearchDocument {
    reference: MemoryRecallQuestionRef,
    search_text: String,
    parts: Vec<RecallPart>,
    source_parts: Vec<ConversationPart>,
}

fn recall_question_key(reference: &MemoryRecallQuestionRef) -> String {
    format!(
        "{}\0{}\0{}",
        reference.record_kind.as_str(),
        reference.session_id,
        reference.question_id
    )
}

fn recall_locator_key(
    record_kind: MemoryRecordKind,
    session_id: &str,
    question_id: &str,
    turn_id: Option<&str>,
    part_id: Option<&str>,
    block_id: &str,
) -> String {
    format!(
        "{}\0{}\0{}\0{}\0{}\0{}",
        record_kind.as_str(),
        session_id,
        question_id,
        turn_id.unwrap_or_default(),
        part_id.unwrap_or_default(),
        block_id
    )
}

fn merge_recall_search_hit(
    hits: &mut BTreeMap<String, MemoryRecallSearchHit>,
    key: String,
    mut candidate: MemoryRecallSearchHit,
) {
    let Some(existing) = hits.get_mut(&key) else {
        hits.insert(key, candidate);
        return;
    };
    existing.lexical_score = existing.lexical_score.max(candidate.lexical_score);
    existing.semantic_score = existing.semantic_score.max(candidate.semantic_score);
    existing.score = existing
        .lexical_score
        .saturating_mul(RECALL_LEXICAL_WEIGHT)
        .saturating_add(existing.semantic_score);
    if candidate.snippet.len() < existing.snippet.len() {
        std::mem::swap(&mut existing.snippet, &mut candidate.snippet);
    }
    existing.sources.extend(candidate.sources);
    existing.sources.sort();
    existing.sources.dedup();
}

fn document_matches_search_hints(
    document: &RecallSearchDocument,
    params: &MemoryRecallSearchParams,
) -> bool {
    let contains_hint = |hint: Option<&String>, values: Vec<String>| {
        hint.map(|value| value.trim())
            .filter(|hint| !hint.is_empty())
            .is_none_or(|hint| {
                values
                    .iter()
                    .any(|value| value.to_lowercase().contains(&hint.to_lowercase()))
            })
    };
    let values = document
        .source_parts
        .iter()
        .flat_map(|part| {
            [
                part.text.clone(),
                part.command.clone(),
                part.cwd.clone(),
                part.command_label.clone(),
                part.metadata_json.clone(),
            ]
            .into_iter()
            .flatten()
        })
        .collect::<Vec<_>>();
    if !contains_hint(params.file.as_ref(), values.clone()) {
        return false;
    }
    if let Some(command) = params
        .command
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let command = command.to_lowercase();
        if !document.source_parts.iter().any(|part| {
            part.kind == crate::backend::models::ConversationPartKind::Command
                || part.command.is_some()
        }) || !values
            .iter()
            .any(|value| value.to_lowercase().contains(&command))
        {
            return false;
        }
    }
    if let Some(error) = params
        .error
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        let error = error.to_lowercase();
        if !document.source_parts.iter().any(|part| {
            part.exit_code.is_some_and(|code| code != 0)
                || part
                    .status
                    .as_deref()
                    .is_some_and(|status| status.to_lowercase().contains("error"))
                || values
                    .iter()
                    .any(|value| value.to_lowercase().contains(&error))
        }) {
            return false;
        }
    }
    true
}

fn leading_recall_snippet(content: &str) -> String {
    content.chars().take(320).collect()
}

fn recall_evidence_parts(
    detail: &crate::backend::dto::ConversationQuestionDetail,
) -> Vec<RecallPart> {
    let mut result = Vec::new();
    for turn in &detail.turns {
        if !turn.user_text.trim().is_empty() {
            result.push(RecallPart {
                turn_id: Some(turn.id.clone()),
                part_id: None,
                block_id: format!("{}-question", turn.id),
                card_type: "question".to_string(),
                content: turn.user_text.clone(),
            });
        }
    }
    for node in &detail.projected_content_nodes {
        result.push(RecallPart {
            turn_id: Some(node.turn_id.clone()),
            part_id: Some(node.part_id.clone()),
            block_id: node.node_id.clone(),
            card_type: node.node_type.clone(),
            content: node.content.clone(),
        });
    }
    result
}

#[cfg(test)]
pub(crate) fn recall_card_projection_for_test(
    detail: &crate::backend::dto::ConversationQuestionDetail,
) -> Vec<(String, String, String)> {
    recall_evidence_parts(detail)
        .into_iter()
        .filter_map(|part| {
            part.part_id
                .map(|part_id| (part_id, part.card_type, part.content))
        })
        .collect()
}

fn validate_recall_search_params(params: &MemoryRecallSearchParams) -> AppResult<()> {
    let query = params.query.trim();
    if query.is_empty() {
        return Err(AppError::Validation(
            "Memory Recall search requires a query".to_string(),
        ));
    }
    if query.chars().count() > 512 {
        return Err(AppError::Validation(
            "Memory Recall search query must not exceed 512 characters".to_string(),
        ));
    }
    for hint in [
        params.file.as_deref(),
        params.command.as_deref(),
        params.error.as_deref(),
    ]
    .into_iter()
    .flatten()
    {
        if hint.chars().count() > 512 {
            return Err(AppError::Validation(
                "Memory Recall search hints must not exceed 512 characters".to_string(),
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::application::AppService;
    use crate::backend::models::{
        ConversationAdapter, ConversationAdapterKind, ConversationAdapterTrustState,
        ConversationPartKind, ConversationPartRole, ConversationSource, ConversationSourceKind,
        MemoryRecordKind, MemoryScope, NormalizedConversationPart, NormalizedConversationSession,
        NormalizedConversationTurn,
    };
    use uuid::Uuid;

    async fn setup_fixture_service(test_name: &str) -> (AppService, std::path::PathBuf) {
        let root = std::env::temp_dir().join(format!(
            "assetiweave-search-{}-{}",
            test_name,
            Uuid::new_v4()
        ));
        std::fs::create_dir_all(&root).expect("create test root");
        let db_path = root.join("app.db");
        let service = AppService::open_with_db_path(db_path).expect("open test service");
        (service, root)
    }

    async fn insert_test_source_and_session(
        service: &AppService,
        tenant_id: &str,
        source_id: &str,
        session_external_id: &str,
        session_title: &str,
        user_query: &str,
        assistant_reply: &str,
    ) -> (ConversationSource, String) {
        let timestamp = "2026-08-30T22:00:00Z";
        let adapter_id = format!("{source_id}-adapter");
        let adapter = ConversationAdapter {
            id: adapter_id.clone(),
            name: "Test Adapter".to_string(),
            kind: ConversationAdapterKind::External,
            version: "1.0.0".to_string(),
            enabled: true,
            manifest_path: None,
            executable_path: None,
            content_hash: None,
            trusted_hash: None,
            trust_state: ConversationAdapterTrustState::Trusted,
            protocol_version: Some(1),
            capabilities: vec!["read_session".to_string()],
            input_kinds: vec![ConversationSourceKind::Directory],
            card_contract_version: None,
            card_kinds: Vec::new(),
            created_at: timestamp.to_string(),
            updated_at: timestamp.to_string(),
        };
        let source = ConversationSource {
            id: source_id.to_string(),
            adapter_id,
            name: "Test Source".to_string(),
            kind: ConversationSourceKind::Directory,
            location: "/fixture/path".to_string(),
            config_json: None,
            enabled: true,
            last_synced_at: None,
            last_sync_status: None,
            created_at: timestamp.to_string(),
            updated_at: timestamp.to_string(),
        };
        let session = NormalizedConversationSession {
            external_id: session_external_id.to_string(),
            title: Some(session_title.to_string()),
            project_path: None,
            started_at: Some(timestamp.to_string()),
            updated_at: Some(timestamp.to_string()),
            source_locator: Some(format!("fixture://{session_external_id}")),
            source_fingerprint: Some("fingerprint-1".to_string()),
            turns: vec![NormalizedConversationTurn {
                external_id: "turn-1".to_string(),
                turn_index: 0,
                user_text: user_query.to_string(),
                title: None,
                started_at: Some(timestamp.to_string()),
                ended_at: Some(timestamp.to_string()),
                parts: vec![NormalizedConversationPart {
                    role: ConversationPartRole::Assistant,
                    kind: ConversationPartKind::Text,
                    text: Some(assistant_reply.to_string()),
                    language: None,
                    command: None,
                    cwd: None,
                    status: None,
                    exit_code: None,
                    command_label: None,
                    source_execution_id: None,
                    content_card: None,
                    metadata_json: None,
                }],
            }],
        };
        crate::backend::store::upsert_conversation_adapter_sqlx(
            service.db.pool(),
            tenant_id,
            &adapter,
        )
        .await
        .expect("upsert adapter");
        crate::backend::store::upsert_conversation_source_sqlx(
            service.db.pool(),
            tenant_id,
            &source,
        )
        .await
        .expect("upsert source");
        crate::backend::store::import_conversation_sessions_sqlx(
            service.db.pool(),
            tenant_id,
            &source,
            &[session],
            false,
        )
        .await
        .expect("import session");
        let session_id: String = sqlx::query_scalar(
            "SELECT id FROM conversation_sessions WHERE tenant_id = ?1 AND external_id = ?2",
        )
        .bind(tenant_id)
        .bind(session_external_id)
        .fetch_one(service.db.pool())
        .await
        .expect("load session id");
        (source, session_id)
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn search_memory_recall_empty_and_no_result() {
        let (service, root) = setup_fixture_service("empty-no-result").await;

        let empty_params = MemoryRecallSearchParams {
            query: "   ".to_string(),
            scope: MemoryScope::default(),
            since: None,
            until: None,
            file: None,
            command: None,
            error: None,
            limit: None,
            offset: None,
        };
        let err = service
            .search_memory_recall(empty_params)
            .await
            .unwrap_err();
        assert!(matches!(err, AppError::Validation(_)));

        let long_query_params = MemoryRecallSearchParams {
            query: "a".repeat(513),
            scope: MemoryScope::default(),
            since: None,
            until: None,
            file: None,
            command: None,
            error: None,
            limit: None,
            offset: None,
        };
        let err = service
            .search_memory_recall(long_query_params)
            .await
            .unwrap_err();
        assert!(matches!(err, AppError::Validation(_)));

        let long_hint_params = MemoryRecallSearchParams {
            query: "valid query".to_string(),
            scope: MemoryScope::default(),
            since: None,
            until: None,
            file: Some("f".repeat(513)),
            command: None,
            error: None,
            limit: None,
            offset: None,
        };
        let err = service
            .search_memory_recall(long_hint_params)
            .await
            .unwrap_err();
        assert!(matches!(err, AppError::Validation(_)));

        let no_match_params = MemoryRecallSearchParams {
            query: "nonexistent query xyz 123".to_string(),
            scope: MemoryScope::default(),
            since: None,
            until: None,
            file: None,
            command: None,
            error: None,
            limit: None,
            offset: None,
        };
        let result = service.search_memory_recall(no_match_params).await.unwrap();
        assert!(result.hits.is_empty());
        assert_eq!(result.total_count, 0);

        drop(service);
        std::fs::remove_dir_all(root).ok();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn search_memory_recall_locators_and_ranking() {
        let (service, root) = setup_fixture_service("locators-ranking").await;

        let (_source, session_id) = insert_test_source_and_session(
            &service,
            "default",
            "source-migration",
            "session-migration",
            "Database Migration Session",
            "How do we handle database migration?",
            "Execute database migration carefully with schema updates.",
        )
        .await;

        let params = MemoryRecallSearchParams {
            query: "database migration".to_string(),
            scope: MemoryScope::default(),
            since: None,
            until: None,
            file: None,
            command: None,
            error: None,
            limit: Some(10),
            offset: None,
        };
        let result = service.search_memory_recall(params).await.unwrap();
        assert!(
            !result.hits.is_empty(),
            "expected hits for 'database migration'"
        );

        for window in result.hits.windows(2) {
            assert!(window[0].score >= window[1].score);
        }

        let hit = &result.hits[0];
        assert_eq!(hit.record_kind, MemoryRecordKind::Session);
        assert_eq!(hit.session_id, session_id);
        assert!(!hit.question_id.is_empty());
        assert!(!hit.block_id.is_empty());
        assert!(
            hit.snippet.to_lowercase().contains("migration")
                || hit.snippet.to_lowercase().contains("database")
        );

        drop(service);
        std::fs::remove_dir_all(root).ok();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn search_memory_recall_tenant_isolation() {
        let (service, root) = setup_fixture_service("tenant-isolation").await;

        sqlx::query(
            "INSERT OR IGNORE INTO tenants (id, name, created_at, updated_at) VALUES ('tenant-b', 'Tenant B', '2026-08-30T22:00:00Z', '2026-08-30T22:00:00Z')",
        )
        .execute(service.db.pool())
        .await
        .expect("insert tenant-b");

        let (_source, _session_id) = insert_test_source_and_session(
            &service,
            "tenant-b",
            "source-tenant-b",
            "session-tenant-b",
            "Tenant B Exclusive Session",
            "Tenant B special migration secrets",
            "Confidential database migration info for tenant B only.",
        )
        .await;

        let params = MemoryRecallSearchParams {
            query: "Tenant B special migration secrets".to_string(),
            scope: MemoryScope::default(),
            since: None,
            until: None,
            file: None,
            command: None,
            error: None,
            limit: None,
            offset: None,
        };
        let result = service.search_memory_recall(params).await.unwrap();
        assert!(
            result.hits.is_empty(),
            "tenant-b data must not leak into default tenant search"
        );

        drop(service);
        std::fs::remove_dir_all(root).ok();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn search_memory_recall_excluded_records() {
        let (service, root) = setup_fixture_service("excluded-records").await;

        let (source, _session_id) = insert_test_source_and_session(
            &service,
            "default",
            "source-excluded",
            "session-excluded",
            "Database Indexing Optimization",
            "How do we optimize database indexing?",
            "Use B-tree indexing for database optimization.",
        )
        .await;

        let query_params = || MemoryRecallSearchParams {
            query: "database indexing".to_string(),
            scope: MemoryScope::default(),
            since: None,
            until: None,
            file: None,
            command: None,
            error: None,
            limit: None,
            offset: None,
        };

        let initial_result = service.search_memory_recall(query_params()).await.unwrap();
        assert!(
            !initial_result.hits.is_empty(),
            "initial search should find record"
        );

        // (a) Disabled source
        sqlx::query(
            "UPDATE conversation_sources SET enabled = 0 WHERE tenant_id = 'default' AND id = ?1",
        )
        .bind(&source.id)
        .execute(service.db.pool())
        .await
        .expect("disable source");
        let disabled_result = service.search_memory_recall(query_params()).await.unwrap();
        assert!(
            disabled_result.hits.is_empty(),
            "disabled source records must be excluded"
        );

        sqlx::query(
            "UPDATE conversation_sources SET enabled = 1 WHERE tenant_id = 'default' AND id = ?1",
        )
        .bind(&source.id)
        .execute(service.db.pool())
        .await
        .expect("re-enable source");

        // (b) Missing session
        sqlx::query("UPDATE conversation_sessions SET missing = 1 WHERE tenant_id = 'default'")
            .execute(service.db.pool())
            .await
            .expect("mark session missing");
        let missing_result = service.search_memory_recall(query_params()).await.unwrap();
        assert!(
            missing_result.hits.is_empty(),
            "missing session records must be excluded"
        );

        sqlx::query("UPDATE conversation_sessions SET missing = 0 WHERE tenant_id = 'default'")
            .execute(service.db.pool())
            .await
            .expect("unmark session missing");

        // (c) assetiweave-memory-recall source
        sqlx::query("UPDATE conversation_sources SET adapter_id = 'assetiweave-memory-recall' WHERE tenant_id = 'default' AND id = ?1")
            .bind(&source.id)
            .execute(service.db.pool())
            .await
            .expect("set adapter to assetiweave-memory-recall");
        let recall_adapter_result = service.search_memory_recall(query_params()).await.unwrap();
        assert!(
            recall_adapter_result.hits.is_empty(),
            "assetiweave-memory-recall sources must be excluded"
        );

        drop(service);
        std::fs::remove_dir_all(root).ok();
    }
}
