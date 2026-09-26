use std::{
    fs,
    path::{Path, PathBuf},
};

use chrono::Utc;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use super::engine::{ConversationCardQuery, ConversationSearchDocument, DiskConversationIndex};
use crate::backend::{
    domain::ConversationSearchMatches,
    infrastructure::error::{InfraError, InfraResult},
};

#[derive(Clone, Debug)]
pub(crate) struct MaterializedGeneration {
    pub(crate) generation: String,
    pub(crate) generation_path: PathBuf,
    pub(crate) document_count: i64,
    pub(crate) size_bytes: i64,
}

pub(crate) fn conversation_search_index_root(db_path: &Path, tenant_id: &str) -> PathBuf {
    let database_root = db_path.parent().unwrap_or_else(|| Path::new("."));
    let database_hash = format!("{:x}", Sha256::digest(db_path.to_string_lossy().as_bytes()));
    let tenant_hash = format!("{:x}", Sha256::digest(tenant_id.as_bytes()));
    database_root
        .join("conversation-search-index")
        .join(&database_hash[..16])
        .join(&tenant_hash[..16])
}

pub(crate) fn set_private_directory_permissions(path: &Path) -> InfraResult<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

pub(crate) fn ensure_rebuild_not_cancelled(
    cancellation: Option<&tokio_util::sync::CancellationToken>,
) -> InfraResult<()> {
    if cancellation.is_some_and(tokio_util::sync::CancellationToken::is_cancelled) {
        return Err(InfraError::Cancelled(
            "conversation search index rebuild cancelled".to_string(),
        ));
    }
    Ok(())
}

pub(crate) fn directory_size(
    path: &Path,
    cancellation: Option<&tokio_util::sync::CancellationToken>,
) -> InfraResult<i64> {
    let mut size = 0_u64;
    for entry in walkdir::WalkDir::new(path) {
        ensure_rebuild_not_cancelled(cancellation)?;
        let entry = entry.map_err(|error| InfraError::External(error.to_string()))?;
        if entry.file_type().is_file() {
            size = size.saturating_add(
                entry
                    .metadata()
                    .map_err(|error| InfraError::External(error.to_string()))?
                    .len(),
            );
        }
    }
    i64::try_from(size)
        .map_err(|_| InfraError::External("conversation search index size overflow".to_string()))
}

pub(crate) fn cleanup_old_generations(root: &Path, active_generation: &str) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    let mut generations = entries
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .filter(|entry| {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            name.starts_with("generation-") && !name.ends_with(".tmp")
        })
        .collect::<Vec<_>>();
    generations.sort_by_key(|entry| {
        std::cmp::Reverse(
            entry
                .metadata()
                .and_then(|metadata| metadata.modified())
                .ok(),
        )
    });
    let previous = generations
        .iter()
        .find(|entry| entry.file_name() != active_generation)
        .map(|entry| entry.path());
    for entry in generations {
        if entry.file_name() == active_generation || previous.as_ref() == Some(&entry.path()) {
            continue;
        }
        let _ = fs::remove_dir_all(entry.path());
    }
}

pub(crate) async fn materialize_index_generation(
    root: &Path,
    documents: Vec<ConversationSearchDocument>,
    cancellation: Option<&tokio_util::sync::CancellationToken>,
) -> InfraResult<MaterializedGeneration> {
    ensure_rebuild_not_cancelled(cancellation)?;
    fs::create_dir_all(root)?;
    set_private_directory_permissions(root)?;
    let generation = format!("generation-{}", Uuid::new_v4());
    let temporary_path = root.join(format!("{generation}.tmp"));
    let generation_path = root.join(&generation);

    let cancellation_clone = cancellation.cloned();
    let temporary_path_clone = temporary_path.clone();
    let generation_path_clone = generation_path.clone();

    let build_result = tokio::task::spawn_blocking(move || {
        let index = DiskConversationIndex::create(&temporary_path_clone)?;
        set_private_directory_permissions(&temporary_path_clone)?;
        index.replace_documents_with_checkpoint(&documents, &mut || {
            ensure_rebuild_not_cancelled(cancellation_clone.as_ref())
        })?;
        drop(index);
        ensure_rebuild_not_cancelled(cancellation_clone.as_ref())?;
        fs::rename(&temporary_path_clone, &generation_path_clone)?;
        let size_bytes = directory_size(&generation_path_clone, cancellation_clone.as_ref())?;
        let document_count = i64::try_from(documents.len()).map_err(|_| {
            InfraError::External("conversation search document count overflow".to_string())
        })?;
        Ok::<_, InfraError>((document_count, size_bytes))
    })
    .await
    .map_err(|e| InfraError::External(e.to_string()))?;

    match build_result {
        Ok((document_count, size_bytes)) => Ok(MaterializedGeneration {
            generation,
            generation_path,
            document_count,
            size_bytes,
        }),
        Err(err) => {
            let _ = fs::remove_dir_all(&temporary_path);
            let _ = fs::remove_dir_all(&generation_path);
            Err(err)
        }
    }
}

pub(crate) fn search_generation_index(
    index_path: &Path,
    query: &ConversationCardQuery,
) -> InfraResult<ConversationSearchMatches> {
    let index = DiskConversationIndex::open(index_path)?;
    index.search_cards(query)
}

#[cfg(test)]
#[path = "storage_tests.rs"]
mod tests;
