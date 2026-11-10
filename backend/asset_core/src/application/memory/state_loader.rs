//! 项目记忆状态文件的统一读取入口。
//!
//! 所有业务路径（状态服务、Markdown 投影、近期快照、CLI 查询）都必须经由本模块
//! 读取 `.state/projects/<slug>.json`：读取时顺带把未折叠便携锚点（`~/`）引入前的
//! 历史 slug 状态无损收敛进规范文件，避免同一项目分裂成多棵互不相识的累计树。

use crate::backend::application::prelude::*;
use crate::backend::domain::memory::ProjectMemoryState;
use crate::backend::infrastructure::memory::projection::sync_project_markdown;
use crate::backend::infrastructure::memory::storage::MemoryStorage;

/// 读取项目状态，并把未折叠便携锚点的历史 slug 文件自动收敛进规范状态。
///
/// 收敛发生时顺带刷新 `projects/<slug>.md` 投影，保证 Markdown 视图与
/// `.state/projects/<slug>.json` 状态树一致，且不会把历史累计节点漏在旧文件里。
pub(super) fn load_project_state(
    storage: &MemoryStorage,
    slug: &str,
) -> AppResult<Option<ProjectMemoryState>> {
    let (state, absorbed) = storage
        .read_project_state_merged(slug)
        .map_err(AppError::from)?;
    if !absorbed.is_empty() {
        if let Some(state) = &state {
            sync_project_markdown(storage, state).map_err(AppError::from)?;
        }
        tracing::info!(
            slug = %slug,
            absorbed = ?absorbed,
            "Merged legacy project memory state without portable path anchor"
        );
    }
    Ok(state)
}

/// 读取全部项目状态（先收敛历史 slug，再统一加载）。
pub(super) fn load_all_project_states(
    storage: &MemoryStorage,
) -> AppResult<Vec<ProjectMemoryState>> {
    let dir = storage.state_projects_dir();
    // 第一轮先把未折叠锚点的历史 slug 收敛进规范文件（旧文件会被删除），
    // 第二轮再统一按规范 slug 读取，避免同一项目在一次刷新里出现两份状态。
    for stem in state_file_stems(&dir)? {
        let _ = load_project_state(storage, &stem);
    }
    let mut states = Vec::new();
    for stem in state_file_stems(&dir)? {
        match storage.read_project_state(&stem) {
            Ok(Some(state)) => states.push(state),
            Ok(None) => {}
            Err(error) => tracing::warn!(
                slug = %stem,
                error = %error,
                "Skipped unreadable project memory state file"
            ),
        }
    }
    Ok(states)
}

fn state_file_stems(dir: &std::path::Path) -> AppResult<Vec<String>> {
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .map_err(AppError::from)?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().and_then(|s| s.to_str()) == Some("json"))
        .collect();
    entries.sort_by_key(|e| e.path());
    Ok(entries
        .into_iter()
        .filter_map(|e| {
            e.path()
                .file_stem()
                .and_then(|s| s.to_str())
                .map(str::to_string)
        })
        .collect())
}
