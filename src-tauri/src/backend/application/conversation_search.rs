use super::prelude::*;
use crate::backend::dto::ConversationSearchIndexRebuildReport;
use crate::backend::dto::ConversationSearchIndexStatus;

impl AppService {
    pub(crate) fn rebuild_conversation_search_index(
        &self,
    ) -> AppResult<ConversationSearchIndexRebuildReport> {
        crate::backend::search::conversation::rebuild_conversation_search_index(
            &self.db,
            &self.db_path,
            self.tenant_id(),
        )
    }

    pub(crate) fn get_conversation_search_index_status(
        &self,
    ) -> AppResult<ConversationSearchIndexStatus> {
        let pool = self.db.pool().clone();
        let tenant_id = self.tenant_id().to_string();
        let state = self.db.block_on(async move {
            crate::backend::store::load_or_create_conversation_search_index_state_sqlx(
                &pool, &tenant_id,
            )
            .await
        })?;

        Ok(status_from_state(state))
    }
}

fn status_from_state(
    state: crate::backend::store::ConversationSearchIndexState,
) -> ConversationSearchIndexStatus {
    let supported_modes = state.supported_modes();
    let is_rebuilding = state.lease_owner.is_some();
    let compatible = state.is_compatible();
    ConversationSearchIndexStatus {
        health: if compatible {
            state.health.as_str().to_string()
        } else {
            "failed".to_string()
        },
        schema_version: state.schema_version,
        tokenizer_version: state.tokenizer_version,
        source_revision: state.source_revision,
        indexed_revision: state.indexed_revision,
        active_generation: state.active_generation,
        document_count: state.document_count,
        size_bytes: state.size_bytes,
        last_built_at: state.last_built_at,
        last_error: state.last_error.or_else(|| {
            (!compatible).then(|| {
                "conversation search index schema or tokenizer version is incompatible".to_string()
            })
        }),
        lease_owner: state.lease_owner,
        lease_expires_at: state.lease_expires_at,
        is_rebuilding,
        updated_at: state.updated_at,
        supported_modes,
    }
}
