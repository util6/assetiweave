use std::{collections::HashMap, fs, path::PathBuf};

use thiserror::Error;

use crate::domain::account::{AccountProvider, AgentAccount, AuthType};

#[derive(Debug, Error)]
pub enum SandboxError {
    #[error("failed to create sandbox directory: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid account state: {0}")]
    InvalidAccount(String),
}

#[derive(Clone, Debug)]
pub struct SandboxContext {
    pub sandbox_root: PathBuf,
}

impl SandboxContext {
    pub fn new(sandbox_root: impl Into<PathBuf>) -> Self {
        Self {
            sandbox_root: sandbox_root.into(),
        }
    }

    pub fn default_system_root() -> PathBuf {
        if let Ok(env_home) = std::env::var("ASSETIWEAVE_HOME") {
            let p = PathBuf::from(env_home);
            if !p.as_os_str().is_empty() {
                return p.join("sandboxes").join("profiles");
            }
        }
        dirs::home_dir()
            .map(|h| h.join(".assetiweave").join("sandboxes").join("profiles"))
            .unwrap_or_else(|| PathBuf::from("./sandboxes/profiles"))
    }

    pub fn prepare_profile_sandbox(
        &self,
        account: &AgentAccount,
    ) -> Result<PreparedSandbox, SandboxError> {
        let provider_name = account.provider.as_str();
        let profile_dir = self.sandbox_root.join(provider_name).join(&account.id);
        fs::create_dir_all(&profile_dir)?;

        let mut env_vars = HashMap::new();

        // 1. 根据 Provider 设置各 CLI 专用的配置目录重定向
        match &account.provider {
            AccountProvider::Codex => {
                env_vars.insert(
                    "CODEX_HOME".into(),
                    profile_dir.to_string_lossy().to_string(),
                );
                env_vars.insert(
                    "XDG_CONFIG_HOME".into(),
                    profile_dir.to_string_lossy().to_string(),
                );
            }
            AccountProvider::Claude => {
                env_vars.insert(
                    "CLAUDE_CONFIG_DIR".into(),
                    profile_dir.to_string_lossy().to_string(),
                );
            }
            AccountProvider::Antigravity => {
                env_vars.insert(
                    "ANTIGRAVITY_CONFIG_DIR".into(),
                    profile_dir.to_string_lossy().to_string(),
                );
            }
            AccountProvider::OpenAi => {
                if let Some(key) = &account.credential {
                    env_vars.insert("OPENAI_API_KEY".into(), key.clone());
                }
            }
            AccountProvider::Gemini => {
                if let Some(key) = &account.credential {
                    env_vars.insert("GEMINI_API_KEY".into(), key.clone());
                }
            }
            AccountProvider::Custom(name) => {
                let custom_env_key = format!("{}_CONFIG_DIR", name.to_uppercase());
                env_vars.insert(custom_env_key, profile_dir.to_string_lossy().to_string());
            }
        }

        // 2. 如果存在直接凭据或自定义配置，落盘到沙箱中
        if let Some(cred) = &account.credential {
            if account.auth_type == AuthType::ApiKey {
                let token_file = profile_dir.join("token.txt");
                let _ = fs::write(token_file, cred);
            }
        }

        // 3. 继承用户自定义环境变量覆盖
        for (k, v) in &account.env_overrides {
            env_vars.insert(k.clone(), v.clone());
        }

        Ok(PreparedSandbox {
            profile_dir,
            env_vars,
        })
    }
}

#[derive(Clone, Debug)]
pub struct PreparedSandbox {
    pub profile_dir: PathBuf,
    pub env_vars: HashMap<String, String>,
}

#[cfg(test)]
#[path = "profile_sandbox_tests.rs"]
mod tests;
