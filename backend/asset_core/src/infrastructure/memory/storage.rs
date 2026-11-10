use crate::backend::domain::memory::evidence::redact_memory_text;
use crate::backend::domain::memory::{ProjectMemoryState, RecentMemoryState};
use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct MemoryStorage {
    root_dir: PathBuf,
    tenant: String,
    /// 未折叠便携锚点时的历史 slug 前缀（如 `users-util6`）。
    ///
    /// 由调用方（`AppService::memory_storage`）从宿主机主目录确定性推导后注入。
    /// 为 `None` 时不做任何历史状态收敛，绝不使用模糊后缀匹配去猜，
    /// 避免把别的项目状态误判成本项目历史并删除。
    legacy_slug_prefix: Option<String>,
}

impl MemoryStorage {
    /// 绑定到显式根目录的记忆存储。
    ///
    /// 应用层唯一入口是 `AppService::memory_storage()`：根目录由运行时配置决定
    /// （生产指向用户主目录，测试/临时库指向数据库同级的隔离目录），
    /// 严禁在业务代码里自行猜测路径。
    pub fn new_with_root(base_dir: PathBuf, tenant: &str) -> Self {
        let root_dir = base_dir.join(tenant);
        Self {
            root_dir,
            tenant: tenant.to_string(),
            legacy_slug_prefix: None,
        }
    }

    /// 注入宿主机主目录对应的历史 slug 前缀，开启未折叠锚点旧状态的自动收敛。
    pub fn with_legacy_slug_prefix(mut self, prefix: Option<String>) -> Self {
        self.legacy_slug_prefix = prefix.filter(|p| !p.trim().is_empty());
        self
    }

    pub fn tenant(&self) -> &str {
        &self.tenant
    }

    pub fn root_dir(&self) -> &Path {
        &self.root_dir
    }

    pub fn sessions_dir(&self) -> PathBuf {
        self.root_dir.join("sessions")
    }

    pub fn recent_dir(&self) -> PathBuf {
        self.root_dir.join("recent")
    }

    pub fn projects_dir(&self) -> PathBuf {
        self.root_dir.join("projects")
    }

    pub fn global_file(&self) -> PathBuf {
        self.root_dir.join("MEMORY.md")
    }

    pub fn state_dir(&self) -> PathBuf {
        self.root_dir.join(".state")
    }

    pub fn state_projects_dir(&self) -> PathBuf {
        self.state_dir().join("projects")
    }

    pub fn state_project_file(&self, slug: &str) -> PathBuf {
        self.state_projects_dir().join(format!("{}.json", slug))
    }

    pub fn state_recent_file(&self) -> PathBuf {
        self.state_dir().join("recent.json")
    }

    /// Initialize directories without sessions/ directory layer.
    pub fn init_dirs(&self) -> std::io::Result<()> {
        fs::create_dir_all(self.state_dir())?;
        fs::create_dir_all(self.state_projects_dir())?;
        fs::create_dir_all(self.recent_dir())?;
        fs::create_dir_all(self.projects_dir())?;
        Ok(())
    }

    /// Read project state JSON from `.state/projects/<slug>.json`.
    pub fn read_project_state(&self, slug: &str) -> std::io::Result<Option<ProjectMemoryState>> {
        let path = self.state_project_file(slug);
        if !path.exists() {
            return Ok(None);
        }
        let content = fs::read_to_string(path)?;
        let state: ProjectMemoryState = serde_json::from_str(&content)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        Ok(Some(state))
    }

    /// Read project state and automatically converge legacy un-folded slug files.
    ///
    /// 便携路径锚点折叠（`~/`）引入之前，同一项目会按宿主机绝对路径生成
    /// `users-<name>-<repo>` 这类旧 slug，导致累计主题树与 `unexplored` 分支分裂。
    /// 这里在读取规范 slug 时，把 `.state/projects/` 下所有以 `-<slug>.json`
    /// 结尾的历史状态无损合并（主题按 `topic_id`、分支按 `node.id` 增量补齐，
    /// 会话水位取最大值），随后清理历史文件，保证同一项目只有一份真相源。
    ///
    /// 返回值第二项是被吸收并清理的历史 slug 列表，调用方可据此再刷新 Markdown 投影。
    pub fn read_project_state_merged(
        &self,
        slug: &str,
    ) -> std::io::Result<(Option<ProjectMemoryState>, Vec<String>)> {
        let canonical = self.read_project_state(slug)?;
        let legacy_slugs = self.legacy_state_slugs(slug)?;
        if legacy_slugs.is_empty() {
            return Ok((canonical, Vec::new()));
        }

        let mut merged = canonical.unwrap_or_else(|| ProjectMemoryState::new(slug));
        let mut absorbed = Vec::new();
        for legacy_slug in &legacy_slugs {
            let Some(legacy_state) = self.read_project_state(legacy_slug)? else {
                continue;
            };
            merged.merge_legacy(legacy_state);
            absorbed.push(legacy_slug.clone());
        }
        if absorbed.is_empty() {
            return Ok((Some(merged), absorbed));
        }

        self.write_project_state_atomic(slug, &merged)?;
        for legacy_slug in &absorbed {
            // 旧状态已无损吸收，删除冗余文件避免下一次读取再次分裂。
            fs::remove_file(self.state_project_file(legacy_slug)).ok();
            fs::remove_file(self.projects_dir().join(format!("{legacy_slug}.md"))).ok();
        }
        Ok((Some(merged), absorbed))
    }

    /// List legacy state slugs derived from the host home prefix.
    ///
    /// 只返回**确定性推导**出的一个候选：`<home-slug>-<canonical-slug>`。
    /// 不做目录后缀扫描——`<任意前缀>-<slug>` 的后缀匹配会把别的项目状态误判为
    /// 本项目历史，随后被合并并物理删除。
    fn legacy_state_slugs(&self, slug: &str) -> std::io::Result<Vec<String>> {
        let Some(prefix) = self.legacy_slug_prefix.as_deref() else {
            return Ok(Vec::new());
        };
        if slug == "unspecified" || slug.starts_with(&format!("{prefix}-")) {
            return Ok(Vec::new());
        }
        let candidate = format!("{prefix}-{slug}");
        if !self.state_project_file(&candidate).exists() {
            return Ok(Vec::new());
        }
        Ok(vec![candidate])
    }

    /// Atomically write project state to `.state/projects/<slug>.json` via same-directory temp rename.
    pub fn write_project_state_atomic(
        &self,
        slug: &str,
        state: &ProjectMemoryState,
    ) -> std::io::Result<PathBuf> {
        let target_path = self.state_project_file(slug);
        let parent = if let Some(p) = target_path.parent() {
            fs::create_dir_all(p)?;
            p.to_path_buf()
        } else {
            let p = self.state_projects_dir();
            fs::create_dir_all(&p)?;
            p
        };

        let json_text = serde_json::to_string_pretty(state)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        let redacted = redact_memory_text(&json_text).text;

        let temp_file_name = format!(".tmp_{}.json", Uuid::new_v4());
        let temp_path = parent.join(temp_file_name);

        fs::write(&temp_path, redacted)?;

        #[cfg(windows)]
        {
            if let Ok(metadata) = fs::metadata(&target_path) {
                let mut perms = metadata.permissions();
                if perms.readonly() {
                    perms.set_readonly(false);
                    let _ = fs::set_permissions(&target_path, perms);
                }
            }
        }

        if let Err(e) = fs::rename(&temp_path, &target_path) {
            fs::remove_file(&temp_path).ok();
            return Err(e);
        }

        Ok(target_path)
    }

    /// Read recent memory state JSON from `.state/recent.json`.
    pub fn read_recent_state(&self) -> std::io::Result<Option<RecentMemoryState>> {
        let path = self.state_recent_file();
        if !path.exists() {
            return Ok(None);
        }
        let content = fs::read_to_string(path)?;
        let state: RecentMemoryState = serde_json::from_str(&content)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        Ok(Some(state))
    }

    /// Atomically write recent memory state to `.state/recent.json` via same-directory temp rename.
    pub fn write_recent_state_atomic(&self, state: &RecentMemoryState) -> std::io::Result<PathBuf> {
        let target_path = self.state_recent_file();
        let parent = self.state_dir();
        fs::create_dir_all(&parent)?;

        let json_text = serde_json::to_string_pretty(state)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        let redacted = redact_memory_text(&json_text).text;

        let temp_file_name = format!(".tmp_{}.json", Uuid::new_v4());
        let temp_path = parent.join(temp_file_name);

        fs::write(&temp_path, redacted)?;

        #[cfg(windows)]
        {
            if let Ok(metadata) = fs::metadata(&target_path) {
                let mut perms = metadata.permissions();
                if perms.readonly() {
                    perms.set_readonly(false);
                    let _ = fs::set_permissions(&target_path, perms);
                }
            }
        }

        if let Err(e) = fs::rename(&temp_path, &target_path) {
            fs::remove_file(&temp_path).ok();
            return Err(e);
        }

        Ok(target_path)
    }

    pub fn write_markdown_atomic(
        &self,
        rel_path: impl AsRef<Path>,
        content: &str,
    ) -> std::io::Result<PathBuf> {
        let target_path = self.root_dir.join(rel_path.as_ref());
        if let Some(parent) = target_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let redacted = redact_memory_text(content).text;

        // Atomic write via temp file in the same parent directory
        let parent = target_path
            .parent()
            .unwrap_or_else(|| Path::new(&self.root_dir));
        let temp_file_name = format!(".tmp_{}.md", Uuid::new_v4());
        let temp_path = parent.join(temp_file_name);

        fs::write(&temp_path, redacted)?;
        if let Err(e) = fs::rename(&temp_path, &target_path) {
            fs::remove_file(&temp_path).ok();
            return Err(e);
        }

        Ok(target_path)
    }

    pub fn read_markdown(&self, rel_path: impl AsRef<Path>) -> std::io::Result<Option<String>> {
        let target_path = self.root_dir.join(rel_path.as_ref());
        if !target_path.exists() {
            return Ok(None);
        }
        let content = fs::read_to_string(target_path)?;
        Ok(Some(content))
    }
}

#[cfg(test)]
#[path = "storage_tests.rs"]
mod tests;
