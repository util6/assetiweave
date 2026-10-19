use agent_engine::domain::market::types::AgentMarketProtocol;
use anyhow::{Context, Result};
use asset_core::application::AppService;
use asset_core::infrastructure::agent_market::{
    AgentInstallPreviewRequest, AgentInstallStartRequest, AgentMarketListRequest,
    AgentUninstallStartRequest,
};
use serde_json::Value;

pub struct AgentOperations<'a> {
    app_service: &'a AppService,
}

impl<'a> AgentOperations<'a> {
    pub fn new(app_service: &'a AppService) -> Self {
        Self { app_service }
    }

    pub async fn list_market(
        &self,
        query: Option<String>,
        protocol: Option<String>,
        installed_only: bool,
    ) -> Result<Value> {
        let parsed_protocol = match protocol.as_deref() {
            Some("acp") => Some(AgentMarketProtocol::Acp),
            Some("native") => Some(AgentMarketProtocol::Native),
            _ => None,
        };
        let req = AgentMarketListRequest {
            query,
            protocol: parsed_protocol,
            installed_only,
        };
        let result = self
            .app_service
            .list_agent_market(req)
            .await
            .context("failed to list agent market items")?;
        Ok(serde_json::to_value(result)?)
    }

    pub async fn inspect_market(&self, agent_id: String) -> Result<Value> {
        let result = self
            .app_service
            .inspect_agent_market_item(agent_id)
            .await
            .context("failed to inspect agent market item")?;
        Ok(serde_json::to_value(result)?)
    }

    pub async fn refresh_market(&self) -> Result<Value> {
        let result = self
            .app_service
            .refresh_agent_market_catalog()
            .context("failed to refresh agent market catalog")?;
        Ok(serde_json::to_value(result)?)
    }

    pub async fn list_installed(&self) -> Result<Value> {
        let result = self
            .app_service
            .list_installed_agents()
            .await
            .context("failed to list installed agents")?;
        Ok(serde_json::to_value(result)?)
    }

    pub async fn get_installed(&self, agent_id: String) -> Result<Value> {
        let result = self
            .app_service
            .get_installed_agent(agent_id)
            .await
            .context("failed to get installed agent")?;
        Ok(serde_json::to_value(result)?)
    }

    pub async fn preview_install(
        &self,
        agent_id: String,
        distribution_id: Option<String>,
        action: String,
    ) -> Result<Value> {
        let req = AgentInstallPreviewRequest {
            agent_id,
            distribution_id,
            action,
            agent_version: None,
            catalog_version: None,
        };
        let result = self
            .app_service
            .preview_agent_installation(req)
            .await
            .context("failed to preview agent installation")?;
        Ok(serde_json::to_value(result)?)
    }

    pub async fn run_install(
        &self,
        agent_id: String,
        distribution_id: Option<String>,
        action: String,
        yes: bool,
    ) -> Result<Value> {
        let preview_val = self
            .preview_install(agent_id.clone(), distribution_id.clone(), action.clone())
            .await?;
        if !yes {
            return Ok(preview_val);
        }

        let preview_token = preview_val
            .get("previewToken")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let catalog_version = preview_val
            .get("catalogVersion")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let target_version = preview_val
            .get("targetVersion")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let selected_dist = preview_val
            .get("selectedDistribution")
            .and_then(|d| d.get("distributionId"))
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();

        let req = AgentInstallStartRequest {
            agent_id,
            action,
            catalog_version,
            agent_version: target_version,
            distribution_id: selected_dist,
            preview_token,
        };
        let result = self
            .app_service
            .install_agent(req)
            .await
            .context("failed to run agent installation")?;
        Ok(serde_json::to_value(result)?)
    }

    pub async fn uninstall(&self, agent_id: String, yes: bool) -> Result<Value> {
        if !yes {
            let preview = self
                .app_service
                .preview_agent_uninstall(agent_id)
                .await
                .context("failed to preview agent uninstall")?;
            return Ok(serde_json::to_value(preview)?);
        }
        let req = AgentUninstallStartRequest {
            agent_id,
            clear_capability_assignments: Vec::new(),
            preview_token: "".to_string(),
        };
        let result = self
            .app_service
            .uninstall_agent(req)
            .await
            .context("failed to uninstall agent")?;
        Ok(serde_json::to_value(result)?)
    }

    pub async fn enable(&self, agent_id: String) -> Result<Value> {
        let result = self
            .app_service
            .set_agent_enabled(agent_id, true)
            .await
            .context("failed to enable agent")?;
        Ok(serde_json::to_value(result)?)
    }

    pub async fn disable(&self, agent_id: String) -> Result<Value> {
        let result = self
            .app_service
            .set_agent_enabled(agent_id, false)
            .await
            .context("failed to disable agent")?;
        Ok(serde_json::to_value(result)?)
    }

    pub async fn check(&self, agent_id: Option<String>) -> Result<Value> {
        if let Some(id) = agent_id {
            let result = self
                .app_service
                .check_agent_runtime(id)
                .await
                .context("failed to check agent runtime")?;
            Ok(serde_json::to_value(result)?)
        } else {
            let installed = self.list_installed().await?;
            Ok(installed)
        }
    }
}
