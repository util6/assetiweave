use crate::backend::domain::memory::evidence::redact_memory_text;
use std::fs;
use std::path::{Path, PathBuf};
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct MemoryStorage {
    root_dir: PathBuf,
    tenant: String,
}

impl MemoryStorage {
    pub fn new(tenant: &str) -> Self {
        let base = if let Ok(custom) = std::env::var("ASSETIWEAVE_MEMORY_ROOT") {
            PathBuf::from(custom)
        } else if let Some(home) = dirs::home_dir() {
            home.join(".assetiweave").join("memories")
        } else {
            PathBuf::from("/tmp/assetiweave/memories")
        };
        Self::new_with_root(base, tenant)
    }

    pub fn new_with_root(base_dir: PathBuf, tenant: &str) -> Self {
        let root_dir = base_dir.join(tenant);
        Self {
            root_dir,
            tenant: tenant.to_string(),
        }
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

    pub fn init_dirs(&self) -> std::io::Result<()> {
        fs::create_dir_all(self.sessions_dir())?;
        fs::create_dir_all(self.recent_dir())?;
        fs::create_dir_all(self.projects_dir())?;
        Ok(())
    }

    /// 清理磁盘记忆目录中的所有生成产物，并重新初始化干净的基础分层目录结构
    pub fn clear_all(&self) -> std::io::Result<()> {
        if self.root_dir.exists() {
            for entry in fs::read_dir(&self.root_dir)? {
                let entry = entry?;
                let path = entry.path();
                let file_type = entry.file_type()?;
                if file_type.is_dir() {
                    fs::remove_dir_all(&path)?;
                } else {
                    #[cfg(windows)]
                    {
                        if let Ok(metadata) = fs::metadata(&path) {
                            let mut perms = metadata.permissions();
                            if perms.readonly() {
                                perms.set_readonly(false);
                                let _ = fs::set_permissions(&path, perms);
                            }
                        }
                    }
                    fs::remove_file(&path)?;
                }
            }
        }
        self.init_dirs()?;
        Ok(())
    }

    /// 重置磁盘记忆目录（同 `clear_all`）
    pub fn reset(&self) -> std::io::Result<()> {
        self.clear_all()
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
