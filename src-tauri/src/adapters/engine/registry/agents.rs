//! Engine 命令注册表：Agents 领域

use super::dispatch::*;
use super::types::*;
use crate::adapters::engine::protocol;
use crate::backend::application::AppService;
use crate::{command, param};
use serde_json::{json, Value};

pub(super) const COMMANDS: &[CommandSpec] = &[
    command!(
        "prompt.optimization.run",
        "prompt.optimization.run",
        "Optimize a prompt through the configured AI Agent",
        Write,
        Friendly,
        false,
        crate::backend::application::conversations::card_translation::PromptOptimizationRequest,
        ServiceAsync => |service, params| service.optimize_prompt(params).await,
        &[
            param!("provider", "AI provider family"),
            param!("cli", "CLI Agent when provider is cli"),
            param!("model", "Optional model identifier"),
            param!("prompt", "Rendered prompt optimization instruction")
        ],
        None
    ),
    command!(
        "prompt.optimization.availability",
        "prompt.optimization.availability",
        "Check the Agent assigned to prompt optimization",
        Read,
        Friendly,
        false,
        NoParams,
        Service => |service, _params| service.check_prompt_optimization_availability(),
        &[],
        None
    ),
    command!(
        "list_agent_catalog",
        "agent.catalog.list",
        "List built-in Agent runtime definitions",
        Read,
        App,
        false,
        NoParams,
        Service => |service, _params| service.list_agent_catalog(),
        &[],
        None
    ),
    command!(
        "list_agent_market",
        "agent.market.list",
        "List curated Agent Market items and installation status",
        Read,
        App,
        false,
        crate::backend::infrastructure::agent_market::AgentMarketListRequest,
        ServiceAsync => |service, params| service.list_agent_market(params).await,
        &[
            param!("query", "Optional Agent search query"),
            param!("protocol", "Optional Agent protocol filter"),
            param!("installedOnly", "Only return installed Agents", ["installed_only"])
        ],
        Some("assetiweave-cli agent market list")
    ),
    command!(
        "inspect_agent_market_item",
        "agent.market.inspect",
        "Inspect one curated Agent Market item",
        Read,
        App,
        false,
        AgentMarketInspectParams,
        ServiceAsync => |service, params| service.inspect_agent_market_item(params.agent_id).await,
        &[param!("agentId", "Curated Agent identifier", ["agent_id"])],
        Some("assetiweave-cli agent market inspect")
    ),
    command!(
        "refresh_agent_market",
        "agent.market.refresh.run",
        "Refresh the controlled curated Agent Market catalog",
        HighRiskWrite,
        App,
        false,
        NoParams,
        Service => |service, _params| service.refresh_agent_market_catalog(),
        &[],
        Some("assetiweave-cli agent market refresh")
    ),
    command!(
        "preview_agent_installation",
        "agent.install.preview",
        "Preview an Agent installation, update or reinstall",
        Read,
        App,
        false,
        crate::backend::infrastructure::agent_market::AgentInstallPreviewRequest,
        ServiceAsync => |service, params| service.preview_agent_installation(params).await,
        &[
            param!("agentId", "Curated Agent identifier", ["agent_id"]),
            param!("distributionId", "Optional distribution identifier", ["distribution_id"]),
            param!("action", "install, update or reinstall")
        ],
        Some("assetiweave-cli agent install preview")
    ),
    command!(
        "list_installed_agents",
        "agent.installed.list",
        "List current tenant Agent installations",
        Read,
        App,
        false,
        NoParams,
        ServiceAsync => |service, _params| service.list_installed_agents().await,
        &[],
        Some("assetiweave-cli agent installed")
    ),
    command!(
        "get_installed_agent",
        "agent.installed.get",
        "Get one current tenant Agent installation",
        Read,
        App,
        false,
        AgentInstalledGetParams,
        ServiceAsync => |service, params| service.get_installed_agent(params.agent_id).await,
        &[param!("agentId", "Installed Agent identifier", ["agent_id"])],
        Some("assetiweave-cli agent installed get")
    ),
    command!(
        "preview_agent_uninstall",
        "agent.uninstall.preview",
        "Preview Agent references, ownership and cleanup scope",
        Read,
        App,
        false,
        AgentUninstallPreviewParams,
        ServiceAsync => |service, params| service.preview_agent_uninstall(params.agent_id).await,
        &[param!("agentId", "Installed Agent identifier", ["agent_id"])],
        Some("assetiweave-cli agent uninstall preview")
    ),
    command!(
        "install_agent",
        "agent.install.run",
        "Install or update one Agent from a confirmed preview",
        HighRiskWrite,
        Friendly,
        false,
        crate::backend::infrastructure::agent_market::AgentInstallStartRequest,
        ServiceAsync => |service, params| service.install_agent(params).await,
        &[
            param!("agentId", "Curated Agent identifier", ["agent_id"]),
            param!("action", "install, update or reinstall"),
            param!("catalogVersion", "Catalog version", ["catalog_version"]),
            param!("agentVersion", "Fixed Agent version", ["agent_version"]),
            param!("distributionId", "Selected distribution", ["distribution_id"]),
            param!("previewToken", "Preview confirmation token", ["preview_token"])
        ],
        Some("assetiweave-cli agent install")
    ),
    command!(
        "uninstall_agent",
        "agent.uninstall.run",
        "Uninstall or unbind one Agent",
        HighRiskWrite,
        Friendly,
        false,
        crate::backend::infrastructure::agent_market::AgentUninstallStartRequest,
        ServiceAsync => |service, params| service.uninstall_agent(params).await,
        &[
            param!("agentId", "Installed Agent identifier", ["agent_id"]),
            param!("clearCapabilityAssignments", "Explicit capability assignments to clear", ["clear_capability_assignments"]),
            param!("previewToken", "Preview confirmation token", ["preview_token"])
        ],
        Some("assetiweave-cli agent uninstall")
    ),
    command!(
        "enable_agent",
        "agent.enable",
        "Enable an installed Agent and reload its runtime definition",
        Write,
        App,
        false,
        AgentToggleParams,
        ServiceAsync => |service, params| service.set_agent_enabled(params.agent_id, true).await,
        &[param!("agentId", "Installed Agent identifier", ["agent_id"])],
        Some("assetiweave-cli agent enable <agent-id>")
    ),
    command!(
        "disable_agent",
        "agent.disable",
        "Disable an installed Agent and remove it from the runtime registry",
        Write,
        App,
        false,
        AgentToggleParams,
        ServiceAsync => |service, params| service.set_agent_enabled(params.agent_id, false).await,
        &[param!("agentId", "Installed Agent identifier", ["agent_id"])],
        Some("assetiweave-cli agent disable <agent-id>")
    ),
    command!(
        "check_agent_connection",
        "agent.connection.check",
        "Check Agent installation or ACP connection",
        Read,
        App,
        false,
        crate::backend::application::agents::AgentConnectionCheckRequest,
        ServiceAsync => |service, params| service.check_agent_connection(params).await,
        &[
            param!("agent_id", "Registered Agent identifier", ["agentId"]),
            param!("mode", "Probe mode: installation or connection")
        ],
        None
    ),
    command!(
        "check_agent_runtime",
        "agent.runtime.check",
        "Check one installed Agent runtime entry",
        Read,
        App,
        false,
        AgentRuntimeCheckParams,
        ServiceAsync => |service, params| service.check_agent_runtime(params.agent_id).await,
        &[param!("agentId", "Installed Agent identifier", ["agent_id"])],
        Some("assetiweave-cli agent check")
    ),
    command!(
        "list_agent_models",
        "agent.models.list",
        "Load selectable models advertised by an Agent ACP session",
        Read,
        App,
        false,
        crate::backend::application::agents::AgentModelsRequest,
        ServiceAsync => |service, params| service.list_agent_models(params).await,
        &[
            param!("agent_id", "Registered Agent identifier", ["agentId"])
        ],
        None
    ),
    command!(
        "cancel_agent_model_probe",
        "agent.models.probe.cancel",
        "Cancel an in-flight Agent ACP model probe",
        Write,
        App,
        false,
        AgentToggleParams,
        ServiceAsync => |service, params| service.cancel_agent_model_probe(params.agent_id).await,
        &[param!("agentId", "Registered Agent identifier", ["agent_id"])],
        None
    ),
    command!(
        "optimize_prompt",
        "prompt.optimization.run",
        "Optimize a prompt through the configured AI Agent",
        Write,
        App,
        false,
        crate::backend::application::conversations::card_translation::PromptOptimizationRequest,
        ServiceAsync => |service, params| service.optimize_prompt(params).await,
        &[
            param!("provider", "AI provider family"),
            param!("cli", "CLI Agent when provider is cli"),
            param!("model", "Optional model identifier"),
            param!("prompt", "Rendered prompt optimization instruction")
        ],
        None
    ),
    command!(
        "check_prompt_optimization_availability",
        "prompt.optimization.availability",
        "Check the Agent assigned to prompt optimization",
        Read,
        App,
        false,
        NoParams,
        Service => |service, _params| service.check_prompt_optimization_availability(),
        &[],
        None
    ),
];
