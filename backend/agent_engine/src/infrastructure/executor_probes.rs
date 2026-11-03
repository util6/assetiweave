use std::{sync::Arc, time::Duration};

use crate::domain::{AgentDefinition, AgentId};

use super::{
    backends::{
        acp_models_cache::{AgentProbeIdentity, AgentProbeReport, ProbeFlight},
        acp_probe_report::{
            AcpModelDiscoveryOutcome, AcpProbeReport, AcpProtocolConnectionOutcome,
        },
    },
    executor::AgentExecutor,
    AgentModelsResult, AiExecutionError,
};

impl AgentExecutor {
    pub async fn probe_agent_report(
        &self,
        agent_id: &AgentId,
        force_refresh: bool,
    ) -> Result<Arc<AcpProbeReport>, AiExecutionError> {
        let Some(definition) = self.registry.get(agent_id) else {
            return Err(AiExecutionError::AgentNotFound {
                agent_id: agent_id.clone(),
            });
        };

        let identity = AgentProbeIdentity::from_definition(&definition);

        let (flight, leader) = {
            let mut flights = self.probe_flights.lock().await;
            if !force_refresh && flights.contains_key(&identity) {
                let flight = flights.get(&identity).unwrap();
                flight
                    .callers
                    .fetch_add(1, std::sync::atomic::Ordering::AcqRel);
                (Arc::clone(flight), false)
            } else {
                let flight = Arc::new(ProbeFlight {
                    result: tokio::sync::Mutex::new(None),
                    notify: tokio::sync::Notify::new(),
                    cancellation: super::AiExecutionCancellation::default(),
                    callers: std::sync::atomic::AtomicUsize::new(1),
                    persist_health: std::sync::atomic::AtomicBool::new(false),
                });
                flights.insert(identity.clone(), Arc::clone(&flight));
                (flight, true)
            }
        };

        if leader {
            let _permit =
                self.probe_semaphore
                    .acquire()
                    .await
                    .map_err(|_| AiExecutionError::Protocol {
                        operation: "probe_rate_limit",
                    })?;
            let mut report = self
                .acp
                .probe_connection_and_models(&definition, flight.cancellation.clone())
                .await;

            if let AcpProtocolConnectionOutcome::Failed { error_code, .. } =
                &report.protocol_connection
            {
                if crate::infrastructure::backends::acp_guard::is_transient_probe_failure(
                    error_code,
                ) && !flight.cancellation.is_cancelled()
                {
                    tokio::time::sleep(Duration::from_millis(200)).await;
                    if !flight.cancellation.is_cancelled() {
                        report = self
                            .acp
                            .probe_connection_and_models(&definition, flight.cancellation.clone())
                            .await;
                    }
                }
            }

            if let AcpModelDiscoveryOutcome::Success { .. } = &report.model_discovery {
                let models_result = report.to_models_result(agent_id.as_str());
                self.models_cache.write().await.insert(
                    agent_id.to_string(),
                    identity.clone(),
                    models_result,
                );
            }

            let report_arc = Arc::new(report);
            *flight.result.lock().await = Some(Ok(Arc::clone(&report_arc)));
            flight.notify.notify_waiters();
            self.probe_flights.lock().await.remove(&identity);
            Ok(report_arc)
        } else {
            loop {
                if let Some(result) = flight.result.lock().await.clone() {
                    return result.map_err(|_| AiExecutionError::Protocol {
                        operation: "shared_probe_error",
                    });
                }
                flight.notify.notified().await;
            }
        }
    }

    pub async fn probe_agent(&self, agent_id: &AgentId, force_refresh: bool) -> AgentProbeReport {
        match self.probe_agent_report(agent_id, force_refresh).await {
            Ok(report) => AgentProbeReport::from_acp_report(agent_id, &report),
            Err(err) => AgentProbeReport::unavailable(
                agent_id,
                err.to_view().code.as_str(),
                &err.to_string(),
            ),
        }
    }

    pub async fn get_or_refresh_models(&self, agent_id: &AgentId) -> AgentModelsResult {
        let Some(definition) = self.registry.get(agent_id) else {
            return AgentModelsResult {
                agent_id: agent_id.to_string(),
                available: false,
                current_model_id: None,
                models: Vec::new(),
                error_code: Some("agent_not_found".to_string()),
                error: Some("The selected AI agent is not registered.".to_string()),
            };
        };

        let identity = AgentProbeIdentity::from_definition(&definition);
        if let Some(cached) = self
            .models_cache
            .write()
            .await
            .get(agent_id.as_str(), &identity)
        {
            return cached;
        }

        match self.probe_agent_report(agent_id, false).await {
            Ok(report) => report.to_models_result(agent_id.as_str()),
            Err(err) => AgentModelsResult {
                agent_id: agent_id.to_string(),
                available: false,
                current_model_id: None,
                models: Vec::new(),
                error_code: Some(err.to_view().code.to_string()),
                error: Some(err.to_string()),
            },
        }
    }

    pub async fn invalidate_model_cache(&self, agent_id: &AgentId) {
        self.models_cache.write().await.remove(agent_id.as_str());
        let flights = self.probe_flights.lock().await;
        for (identity, flight) in flights.iter() {
            if identity.agent_id == agent_id.as_str() {
                flight.cancellation.cancel();
            }
        }
    }

    pub async fn invalidate_all_model_cache(&self) {
        self.models_cache.write().await.clear();
        let flights = self.probe_flights.lock().await;
        for flight in flights.values() {
            flight.cancellation.cancel();
        }
    }

    pub async fn cancel_probe(&self, agent_id: &AgentId) -> bool {
        let flights = self.probe_flights.lock().await;
        let mut cancelled = false;
        for (identity, flight) in flights.iter() {
            if identity.agent_id == agent_id.as_str() {
                flight.cancellation.cancel();
                cancelled = true;
            }
        }
        drop(flights);
        self.models_cache.write().await.remove(agent_id.as_str());
        cancelled
    }

    pub async fn probe_connection_and_models(
        &self,
        definition: &AgentDefinition,
        cancellation: super::AiExecutionCancellation,
    ) -> AcpProbeReport {
        self.acp
            .probe_connection_and_models(definition, cancellation)
            .await
    }
}
