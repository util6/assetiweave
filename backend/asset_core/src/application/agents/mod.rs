pub mod agent;
pub mod agent_market;
pub mod agent_market_lifecycle;
pub mod agent_market_types;
pub mod composition;
pub mod lifecycle;
pub mod migration;
pub mod session_item_view;
pub mod session_view;

pub use crate::backend::domain::agents::session::{
    AgentInfoView, AgentSessionContextView, AgentSessionRef, AgentSessionTerminalView,
};
pub use crate::backend::domain::agents::AgentCatalogEntry;
pub use crate::backend::infrastructure::agent_execution::{
    AgentConnectionResult, AgentModelOption, AgentModelsResult,
};
pub use agent::{AgentConnectionCheckRequest, AgentModelsRequest};
pub use composition::resolve_agent_for;
pub use lifecycle::AgentLifecycleCoordinator;
pub use migration::migrate_legacy_assignments;
pub use session_view::{
    AgentSessionGetParams, AgentSessionGetResult, AgentSessionUnavailableView, AgentSessionView,
};
