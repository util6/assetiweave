//! Shared primitives for installable extensions.
//!
//! The kernel owns identity, compatibility, trust gates, process invocation,
//! probing, snapshots, and lifecycle coordination. Conversation and agent
//! manifests remain domain-owned and are intentionally opaque here. A domain
//! package system only supplies kind and inspection; lifecycle side effects
//! stay in the owning domain workflow.

mod error;
mod identity;
mod launcher;
mod lifecycle;
mod registry;
mod trust;

#[allow(unused_imports)]
pub(crate) use error::ExtensionError;
#[allow(unused_imports)]
pub(crate) use identity::{Compatibility, PackageIdentity, PackageKind};
#[allow(unused_imports)]
pub(crate) use launcher::{
    EnvEntry, ExtensionLauncher, InvocationLimits, InvocationResult, ProbeKind, ProbeResult,
    ProbeSpec, ProcessInvocation, RuntimeProgramKind,
};
#[allow(unused_imports)]
pub(crate) use lifecycle::{
    LifecycleOp, LifecycleRequestKey, LifecycleReservationOutcome, LifecycleTaskCoordinator,
    ResourceKey,
};
#[allow(unused_imports)]
pub(crate) use registry::{InspectedPackage, RegistrySnapshot};
#[allow(unused_imports)]
pub(crate) use trust::TrustGate;

use std::path::Path;

use super::runtime::{AppError, WireError};

/// Domain-specific package interpretation stays outside the kernel.
pub(crate) trait DomainPackageSystem: Send + Sync {
    fn kind(&self) -> PackageKind;
    fn inspect(&self, dir: &Path) -> Result<InspectedPackage, ExtensionError>;
}

impl From<ExtensionError> for AppError {
    fn from(error: ExtensionError) -> Self {
        let view = WireError {
            code: error.code().to_string(),
            message: error.public_message(),
            retryable: error.retryable(),
            details: error.details(),
        };
        Self::from(view)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use semver::{Version, VersionReq};
    use std::{path::PathBuf, sync::Arc, thread, time::Duration};

    struct TestTrust {
        enabled: bool,
        changed: bool,
    }

    impl TrustGate for TestTrust {
        fn can_enable(&self) -> bool {
            self.enabled
        }

        fn needs_confirmation(&self) -> bool {
            self.changed
        }

        fn integrity_changed(&self) -> bool {
            self.changed
        }
    }

    #[test]
    fn compatibility_uses_semver_requirement() {
        let compatibility = Compatibility {
            protocol_version: 1,
            core_requirement: Some(VersionReq::parse(">=1.2, <2").unwrap()),
        };
        assert!(compatibility.accepts_core(&Version::parse("1.5.0").unwrap()));
        assert!(!compatibility.accepts_core(&Version::parse("2.0.0").unwrap()));
    }

    #[test]
    fn trust_gate_exposes_changed_state_without_collapsing_it() {
        let changed = TestTrust {
            enabled: false,
            changed: true,
        };
        assert!(!changed.can_enable());
        assert!(changed.needs_confirmation());
        assert!(changed.integrity_changed());
    }

    #[test]
    fn launcher_types_preserve_all_runtime_probe_fields() {
        let invocation = ProcessInvocation {
            kind: RuntimeProgramKind::Python,
            entry: "adapter.py".to_string(),
            args: vec!["--probe".to_string()],
            env: vec![EnvEntry {
                key: "TOKEN".to_string(),
                value: "VALUE".to_string(),
            }],
            working_dir: Some(PathBuf::from("/tmp/adapter")),
            version_req: Some(">=3.11".to_string()),
            immutable_install_dir: PathBuf::from("/tmp/install"),
        };
        let probe = ProbeSpec {
            program: Some("python3".to_string()),
            args: invocation.args.clone(),
            env: invocation.env.clone(),
            timeout: Duration::from_secs(2),
            output_limit: 4096,
            kind: ProbeKind::Availability,
        };
        assert_eq!(invocation.kind, RuntimeProgramKind::Python);
        assert_eq!(probe.output_limit, 4096);
    }

    #[test]
    fn launcher_contract_can_represent_each_supported_runtime_kind() {
        let kinds = [
            RuntimeProgramKind::Node,
            RuntimeProgramKind::Python,
            RuntimeProgramKind::Bash,
            RuntimeProgramKind::Executable,
        ];

        for kind in kinds {
            let invocation = ProcessInvocation {
                kind,
                entry: "entry".to_string(),
                args: vec!["--version".to_string()],
                env: Vec::new(),
                working_dir: None,
                version_req: Some(">=1".to_string()),
                immutable_install_dir: PathBuf::from("/tmp/install"),
            };
            assert_eq!(invocation.args, vec!["--version"]);
            assert_eq!(invocation.version_req.as_deref(), Some(">=1"));
        }
    }

    #[test]
    fn extension_process_error_wire_is_stable_and_does_not_leak_paths() {
        let error = AppError::from(ExtensionError::OutputLimitExceeded {
            package_id: "/Users/util6/private/agent".to_string(),
            stdout: true,
            stderr: false,
        });
        let view = error.view();

        assert_eq!(view.code, "output_limit_exceeded");
        assert!(!view.retryable);
        assert!(!view.message.contains("/Users/util6"));
        assert_eq!(view.details.unwrap()["stdout"], true);
    }

    #[test]
    fn registry_snapshot_replaces_atomically() {
        let snapshot = RegistrySnapshot::new(vec![1_u32]);
        let reader = Arc::new(snapshot);
        reader.replace(vec![1, 2]);
        assert_eq!(&*reader.load(), &[1, 2]);
    }

    #[test]
    fn lifecycle_conflict_matrix_is_symmetric_for_destructive_operations() {
        assert!(LifecycleOp::Install.conflicts_with(LifecycleOp::Remove));
        assert!(LifecycleOp::Remove.conflicts_with(LifecycleOp::Install));
        assert!(LifecycleOp::Disable.conflicts_with(LifecycleOp::Remove));
        assert!(!LifecycleOp::Probe.conflicts_with(LifecycleOp::Probe));
    }

    #[test]
    fn lifecycle_keys_separate_package_kind_and_operation() {
        let version = Version::parse("1.0.0").unwrap();
        let adapter = LifecycleRequestKey {
            resource: ResourceKey::new(PackageIdentity {
                kind: PackageKind::ConversationAdapter,
                package_id: "shared-id".to_string(),
                version: version.clone(),
            }),
            operation: LifecycleOp::Install,
        };
        let agent = LifecycleRequestKey {
            resource: ResourceKey::new(PackageIdentity {
                kind: PackageKind::Agent,
                package_id: "shared-id".to_string(),
                version,
            }),
            operation: LifecycleOp::Install,
        };
        let remove = LifecycleRequestKey {
            resource: adapter.resource.clone(),
            operation: LifecycleOp::Remove,
        };

        assert_ne!(adapter.dedup_key(), agent.dedup_key());
        assert_ne!(adapter.dedup_key(), remove.dedup_key());
        assert!(adapter.conflicts_with(&remove));
        assert!(!adapter.conflicts_with(&agent));
    }

    #[test]
    fn lifecycle_coordinator_deduplicates_exact_operations_and_rejects_conflicts() {
        let key = LifecycleRequestKey {
            resource: ResourceKey::new(PackageIdentity {
                kind: PackageKind::ConversationAdapter,
                package_id: "io.example.adapter".to_string(),
                version: Version::parse("1.0.0").unwrap(),
            }),
            operation: LifecycleOp::Install,
        };
        let coordinator =
            LifecycleTaskCoordinator::new(crate::backend::runtime::tasks::TaskRuntime::new());

        assert!(matches!(
            coordinator.reserve("adapter-task-1".to_string(), key.clone()),
            Ok(LifecycleReservationOutcome::Started)
        ));
        assert!(matches!(
            coordinator.reserve("adapter-task-2".to_string(), key.clone()),
            Ok(LifecycleReservationOutcome::Existing(existing_id))
                if existing_id == "adapter-task-1"
        ));

        let remove = LifecycleRequestKey {
            operation: LifecycleOp::Remove,
            ..key
        };
        let error = coordinator
            .reserve("adapter-task-3".to_string(), remove)
            .expect_err("remove must conflict with an active install");
        assert!(error.to_string().contains("conflicts"));

        let snapshot = coordinator
            .spawn(
                "adapter-task-1",
                serde_json::json!({"domain": "conversation"}),
                Box::new(|_| Ok(serde_json::json!({"ok": true}))),
            )
            .expect("spawn lifecycle task");
        assert_eq!(snapshot.task_id, "adapter-task-1");
        assert_eq!(
            snapshot.kind,
            crate::backend::runtime::tasks::TaskKind::ExtensionLifecycle
        );
    }

    #[test]
    fn registry_snapshot_supports_concurrent_readers_during_replacement() {
        let registry = Arc::new(RegistrySnapshot::new(vec![0_u64]));
        let writer = {
            let registry = registry.clone();
            thread::spawn(move || {
                for value in 1..=100_u64 {
                    registry.replace(vec![value]);
                }
            })
        };
        let readers = (0..4)
            .map(|_| {
                let registry = registry.clone();
                thread::spawn(move || {
                    for _ in 0..100 {
                        let snapshot = registry.load();
                        assert_eq!(snapshot.len(), 1);
                        assert!(snapshot[0] <= 100);
                    }
                })
            })
            .collect::<Vec<_>>();
        writer.join().expect("registry writer");
        for reader in readers {
            reader.join().expect("registry reader");
        }
    }
}
