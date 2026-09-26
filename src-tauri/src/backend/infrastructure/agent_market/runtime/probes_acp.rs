use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Arc,
};

use sha2::{Digest, Sha256};

use crate::backend::{
    domain::agents::{
        AgentInstallation, AgentMarketProtocol, InstallationStatus, ProtocolStatus, RuntimeStatus,
    },
    infrastructure::{
        agent_execution::{
            backends::acp::{
                AcpConnectionStage, AcpExecutionBackend, AcpModelDiscoveryOutcome, AcpProbeReport,
                AcpProtocolConnectionOutcome,
            },
            AgentConnectionResult, AgentModelsResult, AiExecutionCancellation,
        },
        agent_market::error::AgentMarketError,
    },
};

use super::{
    definition::definition_from_installation,
    model_cache::{AgentProbeIdentity, ProbeFlight, SharedProbeError},
    probes_acp_health::*,
    probes_native::unavailable_models,
    AgentRuntimeManager,
};

impl AgentRuntimeManager {
    pub(crate) async fn refresh_acp_connection(
        &self,
        agent_id: &str,
    ) -> Result<AgentConnectionResult, AgentMarketError> {
        let installation = self.current_acp_installation(agent_id).await?;
        if !installation.enabled {
            return Ok(unavailable_acp_connection(
                agent_id,
                "agent_disabled",
                "The ACP Agent is disabled.",
            ));
        }
        if !installation.resolved_program.is_file() {
            return Ok(unavailable_acp_connection(
                agent_id,
                "agent_entry_missing",
                "The resolved Agent entry is missing.",
            ));
        }
        if let Err(error) = definition_from_installation(&installation) {
            return Ok(unavailable_acp_connection(
                agent_id,
                "definition_invalid",
                &error.to_string(),
            ));
        }
        let identity = probe_identity(&installation);
        let report = self.run_acp_probe(agent_id, identity, true, true).await?;
        self.reload_registry().await?;
        let current_inst = self.repository.get(agent_id).await?.unwrap_or(installation);
        Ok(report.to_connection_result(
            agent_id,
            Some(&current_inst.agent_version),
            Some(current_inst.installation_status.as_str()),
            Some(current_inst.runtime_status.as_str()),
            Some(current_inst.protocol_status.as_str()),
        ))
    }

    pub(crate) async fn refresh_acp_health(
        &self,
        agent_id: &str,
    ) -> Result<AgentModelsResult, AgentMarketError> {
        let installation = self.current_acp_installation(agent_id).await?;
        let identity = probe_identity(&installation);
        let report = self.run_acp_probe(agent_id, identity, true, true).await?;
        self.reload_registry().await?;
        Ok(report.to_models_result(agent_id))
    }

    #[cfg(test)]
    pub(crate) async fn probe_acp_health(
        &self,
        agent_id: &str,
    ) -> Result<AgentModelsResult, AgentMarketError> {
        let installation = self.current_acp_installation(agent_id).await?;
        let identity = probe_identity(&installation);
        let report = self.run_acp_probe(agent_id, identity, true, true).await?;
        Ok(report.to_models_result(agent_id))
    }

    pub(crate) async fn get_or_refresh_acp_models(
        &self,
        agent_id: &str,
    ) -> Result<AgentModelsResult, AgentMarketError> {
        let installation = self.current_acp_installation(agent_id).await?;
        if !installation.enabled {
            return Ok(unavailable_models(
                agent_id,
                "agent_disabled",
                "The ACP Agent is disabled.",
            ));
        }
        if !installation.resolved_program.is_file() {
            return Ok(unavailable_models(
                agent_id,
                "agent_entry_missing",
                "The resolved Agent entry is missing.",
            ));
        }
        if let Err(error) = definition_from_installation(&installation) {
            return Ok(unavailable_models(
                agent_id,
                "definition_invalid",
                &error.to_string(),
            ));
        }
        let identity = probe_identity(&installation);
        if let Some(result) = self.cached_acp_models(agent_id, &identity).await {
            return Ok(result);
        }
        let report = self.run_acp_probe(agent_id, identity, false, false).await?;
        Ok(report.to_models_result(agent_id))
    }

    pub(crate) async fn invalidate_agent_state(&self, agent_id: &str) {
        self.models_cache.write().await.remove(agent_id);
        let flights = self.probe_flights.lock().await;
        for (identity, flight) in flights.iter() {
            if identity.agent_id == agent_id {
                flight.cancellation.cancel();
            }
        }
    }

    pub(crate) async fn cancel_acp_probe(&self, agent_id: &str) -> bool {
        let flights = self.probe_flights.lock().await;
        let mut cancelled = false;
        for (identity, flight) in flights.iter() {
            if identity.agent_id == agent_id {
                flight.cancellation.cancel();
                cancelled = true;
            }
        }
        drop(flights);
        self.models_cache.write().await.remove(agent_id);
        cancelled
    }

    pub(crate) async fn release_acp_probe_caller(&self, agent_id: &str) -> bool {
        let flights = self.probe_flights.lock().await;
        let mut cancelled = false;
        for (identity, flight) in flights.iter() {
            if identity.agent_id != agent_id {
                continue;
            }
            let previous = flight.callers.load(Ordering::Acquire);
            if previous > 0 && flight.callers.fetch_sub(1, Ordering::AcqRel) == 1 {
                flight.cancellation.cancel();
                cancelled = true;
            }
        }
        cancelled
    }

    pub(crate) async fn invalidate_all_acp_probe_state(&self) {
        self.models_cache.write().await.clear();
        let flights = self.probe_flights.lock().await;
        for flight in flights.values() {
            flight.cancellation.cancel();
        }
    }

    async fn current_acp_installation(
        &self,
        agent_id: &str,
    ) -> Result<AgentInstallation, AgentMarketError> {
        let installation = self.repository.get(agent_id).await?.ok_or_else(|| {
            AgentMarketError::InstallationNotFound {
                agent_id: agent_id.to_string(),
            }
        })?;
        if installation.protocol != AgentMarketProtocol::Acp {
            return Err(AgentMarketError::new(
                "protocol_mismatch",
                "The installed Agent does not use ACP.",
                false,
            ));
        }
        Ok(installation)
    }

    async fn cached_acp_models(
        &self,
        agent_id: &str,
        identity: &AgentProbeIdentity,
    ) -> Option<AgentModelsResult> {
        let mut cache = self.models_cache.write().await;
        cache.get(agent_id, identity)
    }

    async fn run_acp_probe(
        &self,
        agent_id: &str,
        identity: AgentProbeIdentity,
        _force_refresh: bool,
        persist_health: bool,
    ) -> Result<Arc<AcpProbeReport>, AgentMarketError> {
        let (flight, leader) = {
            let mut flights = self.probe_flights.lock().await;
            if let Some(flight) = flights.get(&identity) {
                flight.callers.fetch_add(1, Ordering::AcqRel);
                if persist_health {
                    flight.persist_health.store(true, Ordering::Release);
                }
                (Arc::clone(flight), false)
            } else {
                let flight = Arc::new(ProbeFlight {
                    result: tokio::sync::Mutex::new(None),
                    notify: tokio::sync::Notify::new(),
                    cancellation: AiExecutionCancellation::default(),
                    callers: AtomicUsize::new(1),
                    persist_health: AtomicBool::new(persist_health),
                });
                flights.insert(identity.clone(), Arc::clone(&flight));
                (flight, true)
            }
        };

        if leader {
            let raw_outcome = self
                .probe_acp_health_uncached(agent_id, identity.clone(), flight.cancellation.clone())
                .await;

            let outcome: Result<Arc<AcpProbeReport>, AgentMarketError> = match raw_outcome {
                Ok(report) => {
                    if matches!(
                        report.protocol_connection,
                        AcpProtocolConnectionOutcome::Cancelled
                    ) {
                        Err(AgentMarketError::new(
                            "cancelled",
                            "The ACP probe was cancelled.",
                            true,
                        )
                        .with_agent_id(agent_id))
                    } else {
                        if flight.persist_health.load(Ordering::Acquire) {
                            self.persist_acp_probe_health(agent_id, &identity, &report)
                                .await;
                        }
                        if let AcpModelDiscoveryOutcome::Success { .. } = &report.model_discovery {
                            let models_result = report.to_models_result(agent_id);
                            self.models_cache.write().await.insert(
                                agent_id.to_string(),
                                identity.clone(),
                                models_result,
                            );
                        }
                        Ok(Arc::new(report))
                    }
                }
                Err(error) => Err(error),
            };

            let shared = outcome
                .as_ref()
                .map(|report| Arc::clone(report))
                .map_err(|error| SharedProbeError::from_error(agent_id, error));
            *flight.result.lock().await = Some(shared);
            flight.notify.notify_waiters();
            self.probe_flights.lock().await.remove(&identity);
            outcome
        } else {
            loop {
                if let Some(result) = flight.result.lock().await.clone() {
                    return result.map_err(SharedProbeError::into_error);
                }
                flight.notify.notified().await;
            }
        }
    }

    async fn probe_acp_health_uncached(
        &self,
        agent_id: &str,
        expected_identity: AgentProbeIdentity,
        cancellation: AiExecutionCancellation,
    ) -> Result<AcpProbeReport, AgentMarketError> {
        if cancellation.is_cancelled() {
            return Err(AgentMarketError::new(
                "cancelled",
                "The ACP probe was cancelled.",
                true,
            ));
        }

        let installation = self.repository.get(agent_id).await?.ok_or_else(|| {
            AgentMarketError::InstallationNotFound {
                agent_id: agent_id.to_string(),
            }
        })?;
        if installation.protocol != AgentMarketProtocol::Acp {
            return Err(AgentMarketError::new(
                "protocol_mismatch",
                "The installed Agent does not use ACP.",
                false,
            ));
        }

        if !installation.enabled {
            return Ok(AcpProbeReport {
                protocol_connection: AcpProtocolConnectionOutcome::Failed {
                    stage: AcpConnectionStage::Spawn,
                    error_code: "agent_disabled".to_string(),
                    error_message: "The ACP Agent is disabled.".to_string(),
                },
                model_discovery: AcpModelDiscoveryOutcome::Skipped,
                cleanup: crate::backend::infrastructure::agent_execution::backends::acp::AcpCleanupOutcome {
                    process_reaped: true,
                    workspace_removed: true,
                    timed_out: false,
                    failures: Vec::new(),
                },
                timings: crate::backend::infrastructure::agent_execution::backends::acp::AcpProbeTimings::default(),
            });
        }

        if !installation.resolved_program.is_file() {
            return Ok(AcpProbeReport {
                protocol_connection: AcpProtocolConnectionOutcome::Failed {
                    stage: AcpConnectionStage::Spawn,
                    error_code: "agent_entry_missing".to_string(),
                    error_message: "The resolved Agent entry is missing.".to_string(),
                },
                model_discovery: AcpModelDiscoveryOutcome::Skipped,
                cleanup: crate::backend::infrastructure::agent_execution::backends::acp::AcpCleanupOutcome {
                    process_reaped: true,
                    workspace_removed: true,
                    timed_out: false,
                    failures: Vec::new(),
                },
                timings: crate::backend::infrastructure::agent_execution::backends::acp::AcpProbeTimings::default(),
            });
        }

        let definition = match definition_from_installation(&installation) {
            Ok(definition) => definition,
            Err(error) => {
                return Ok(AcpProbeReport {
                    protocol_connection: AcpProtocolConnectionOutcome::Failed {
                        stage: AcpConnectionStage::Spawn,
                        error_code: "definition_invalid".to_string(),
                        error_message: error.to_string(),
                    },
                    model_discovery: AcpModelDiscoveryOutcome::Skipped,
                    cleanup: crate::backend::infrastructure::agent_execution::backends::acp::AcpCleanupOutcome {
                        process_reaped: true,
                        workspace_removed: true,
                        timed_out: false,
                        failures: Vec::new(),
                    },
                    timings: crate::backend::infrastructure::agent_execution::backends::acp::AcpProbeTimings::default(),
                });
            }
        };

        let identity = probe_identity(&installation);
        if identity != expected_identity {
            tracing::debug!(
                action = "agent_market.acp_probe.identity_changed",
                agent_id,
                "ACP probe identity changed before the process started"
            );
        }

        let backend = AcpExecutionBackend::new(self.workspace_root.clone());
        let report = backend
            .probe_connection_and_models(&definition, cancellation)
            .await;
        Ok(report)
    }
}
