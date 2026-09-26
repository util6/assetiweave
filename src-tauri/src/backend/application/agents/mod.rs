pub(crate) mod agent;
pub(crate) mod agent_market;
pub(crate) mod agent_market_lifecycle;
pub(crate) mod agent_market_types;
pub(crate) mod composition;
pub(crate) mod lifecycle;
pub(crate) mod migration;
pub(crate) mod session_item_view;
pub(crate) mod session_view;

pub(crate) use crate::backend::domain::agents::session::{
    AgentInfoView, AgentSessionContextView, AgentSessionRef, AgentSessionTerminalView,
};
pub(crate) use crate::backend::domain::agents::AgentCatalogEntry;
pub(crate) use crate::backend::infrastructure::agent_execution::{
    AgentConnectionResult, AgentModelOption, AgentModelsResult,
};
pub(crate) use agent::{AgentConnectionCheckRequest, AgentModelsRequest};
pub(crate) use composition::resolve_agent_for;
pub(crate) use lifecycle::AgentLifecycleCoordinator;
pub(crate) use migration::migrate_legacy_assignments;
pub(crate) use session_view::{
    AgentSessionGetParams, AgentSessionGetResult, AgentSessionUnavailableView, AgentSessionView,
};
