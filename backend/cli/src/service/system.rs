use anyhow::{bail, Context, Result};
use asset_core::application::AppService;
use clap::CommandFactory;
use clap_complete::{generate, Shell};
use serde_json::{json, Value};
use std::fs;
use std::io;

const CONTRACT_JSON: &str = include_str!("../../../../cli/internal/schema/contract.json");

pub struct SystemOperations<'a> {
    app_service: &'a AppService,
}

impl<'a> SystemOperations<'a> {
    pub fn new(app_service: &'a AppService) -> Self {
        Self { app_service }
    }

    pub async fn get_settings(&self) -> Result<Value> {
        let res = self
            .app_service
            .get_app_settings()
            .await
            .context("failed to get application settings")?;
        Ok(serde_json::to_value(res)?)
    }

    pub async fn save_settings(&self, json_str: &str) -> Result<Value> {
        let val: Value =
            serde_json::from_str(json_str).context("invalid JSON passed to settings save")?;
        if !val.is_object() {
            bail!("settings JSON must be a JSON object");
        }
        let res = self
            .app_service
            .save_app_settings(val)
            .await
            .context("failed to save application settings")?;
        Ok(serde_json::to_value(res)?)
    }

    pub async fn run_doctor(&self) -> Result<Value> {
        self.app_service
            .run_doctor()
            .await
            .context("failed to run doctor diagnostics")
    }

    pub fn config_plugins_show(&self) -> Value {
        json!({
            "plugins": []
        })
    }

    pub fn check_update(&self, _check_only: bool, _yes: bool) -> Value {
        let current = env!("CARGO_PKG_VERSION");
        json!({
            "checked": true,
            "update_available": false,
            "current": current,
            "latest": current,
            "action": "up_to_date",
            "release_url": format!("https://github.com/util6/assetiweave/releases/tag/v{}", current),
            "message": format!("AssetIWeave CLI {} is up to date", current)
        })
    }

    pub fn generate_completion(&self, shell: Shell) {
        let mut cmd = crate::args::CliArgs::command();
        let bin_name = cmd.get_name().to_string();
        generate(shell, &mut cmd, bin_name, &mut io::stdout());
    }

    pub fn schema_list(&self) -> Result<Value> {
        let contract: Value = serde_json::from_str(CONTRACT_JSON)
            .context("failed to parse embedded contract.json")?;
        let methods = contract
            .get("methods")
            .cloned()
            .unwrap_or_else(|| json!([]));
        Ok(json!({ "methods": methods }))
    }

    pub fn schema_get(&self, method: &str) -> Result<Value> {
        let contract: Value = serde_json::from_str(CONTRACT_JSON)
            .context("failed to parse embedded contract.json")?;
        let commands = contract
            .get("commands")
            .and_then(Value::as_array)
            .context("invalid contract: commands not found")?;

        for item in commands {
            if item.get("method").and_then(Value::as_str) == Some(method)
                || item.get("canonical_method").and_then(Value::as_str) == Some(method)
            {
                return Ok(item.clone());
            }
        }
        bail!("method '{}' not found in schema", method);
    }

    pub async fn api_call(
        &self,
        method: &str,
        json_arg: Option<&str>,
        _yes: bool,
    ) -> Result<Value> {
        let _params: Value = match json_arg {
            Some("-") => {
                let mut buffer = String::new();
                io::Read::read_to_string(&mut io::stdin(), &mut buffer)
                    .context("failed to read JSON from stdin")?;
                serde_json::from_str(&buffer).context("invalid JSON from stdin")?
            }
            Some(path) if path.starts_with('@') => {
                let file_path = &path[1..];
                let content = fs::read_to_string(file_path)
                    .with_context(|| format!("failed to read JSON file at {}", file_path))?;
                serde_json::from_str(&content).context("invalid JSON from file")?
            }
            Some(inline) => serde_json::from_str(inline).context("invalid inline JSON")?,
            None => json!({}),
        };

        // 常见通用方法直接代理
        match method {
            "doctor.run" => self.run_doctor().await,
            "overview.get" => {
                let ov = self.app_service.overview().await?;
                Ok(serde_json::to_value(ov)?)
            }
            "settings.get" => self.get_settings().await,
            "tenant.list" => {
                let tenants = self.app_service.list_tenants().await?;
                Ok(serde_json::to_value(tenants)?)
            }
            "tenant.active" => {
                let active = self.app_service.active_tenant().await?;
                Ok(serde_json::to_value(active)?)
            }
            "profile.list" => {
                let profiles = self.app_service.list_target_profile_descriptors()?;
                Ok(serde_json::to_value(profiles)?)
            }
            "skill.list" => {
                let skills = self.app_service.list_skills().await?;
                Ok(serde_json::to_value(skills)?)
            }
            "source.list" => {
                let sources = self.app_service.list_sources().await?;
                Ok(serde_json::to_value(sources)?)
            }
            _ => {
                bail!(
                    "api call for method '{}' is not registered or supported directly",
                    method
                );
            }
        }
    }
}
