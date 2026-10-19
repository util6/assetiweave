use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread;

use super::*;
use crate::backend::domain::agents::types::{
    AgentMarketProtocol, CatalogCapabilities, CoreCompatibility, UpstreamSource, Verification,
};

#[test]
fn test_semver_is_newer() {
    assert!(UpstreamProbe::semver_is_newer("2.2.2", "1.6.2"));
    assert!(UpstreamProbe::semver_is_newer("2.2.3", "2.2.2"));
    assert!(!UpstreamProbe::semver_is_newer("1.6.2", "2.2.2"));
    assert!(!UpstreamProbe::semver_is_newer("1.6.2", "1.6.2"));
    assert!(!UpstreamProbe::semver_is_newer(" 1.6.2 ", "1.6.2"));
}

#[test]
fn test_extract_package_name() {
    let npx_item = CatalogItem {
        id: "codex".to_string(),
        display_name: "Codex".to_string(),
        description: "Codex ACP".to_string(),
        protocol: AgentMarketProtocol::Acp,
        version: "1.6.2".to_string(),
        core_compatibility: CoreCompatibility::default(),
        capabilities: CatalogCapabilities::default(),
        verification: Verification {
            status: crate::backend::domain::agents::types::VerificationStatus::Tested,
            tested_at: "2026-10-15T00:00:00Z".to_string(),
            evidence_id: None,
        },
        upstream: UpstreamSource {
            registry_id: "codex-acp".to_string(),
            homepage: String::new(),
            license: String::new(),
        },
        distributions: vec![Distribution::Npx {
            id: "npx-codex".to_string(),
            priority: 10,
            package: "@agentclientprotocol/codex-acp".to_string(),
            version: "1.6.2".to_string(),
            bin: "codex-acp".to_string(),
            launch_args: vec![],
            node_range: None,
            model_discovery_args: None,
            session_cleanup_args: None,
            session_cleanup_not_found_markers: vec![],
        }],
    };

    assert_eq!(
        UpstreamProbe::extract_package_name(&npx_item).as_deref(),
        Some("@agentclientprotocol/codex-acp")
    );
}

#[test]
fn test_cache_save_and_load_roundtrip() {
    let temp_dir =
        std::env::temp_dir().join(format!("upstream-cache-test-{}", uuid::Uuid::new_v4()));
    let cache_path = temp_dir.join("upstream-versions.json");

    let mut map = HashMap::new();
    map.insert(
        "codex".to_string(),
        UpstreamVersionInfo {
            version: "2.2.2".to_string(),
            source: UpstreamProbeSource::AcpRegistry,
            checked_at: "2026-10-15T00:00:00Z".to_string(),
            binary_targets: HashMap::new(),
        },
    );

    UpstreamProbe::save_cached_upstream_versions(&cache_path, &map).expect("save succeeded");
    let loaded = UpstreamProbe::load_cached_upstream_versions(&cache_path);
    assert_eq!(loaded.get("codex"), map.get("codex"));

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_probe_priority_acp_registry_wins() {
    // Spin up a mock local HTTP server that responds with 200 for ACP registry
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind listener");
    let port = listener.local_addr().unwrap().port();
    let base_url = format!("http://127.0.0.1:{port}");

    thread::spawn(move || {
        for stream in listener.incoming() {
            if let Ok(mut stream) = stream {
                let mut buffer = [0; 1024];
                let _ = stream.read(&mut buffer);
                let response = "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{\"version\":\"2.2.2\"}";
                let _ = stream.write_all(response.as_bytes());
                let _ = stream.flush();
                break;
            }
        }
    });

    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_millis(500))
        .build()
        .unwrap();

    let result = UpstreamProbe::probe_agent_with_urls(
        &client,
        Some("codex-acp"),
        Some("@agentclientprotocol/codex-acp"),
        &base_url,
        "http://invalid.local",
        Duration::from_millis(500),
    );

    assert!(result.is_some());
    let info = result.unwrap();
    assert_eq!(info.version, "2.2.2");
    assert_eq!(info.source, UpstreamProbeSource::AcpRegistry);
}

#[test]
fn test_probe_fallback_to_npm_when_acp_fails() {
    // Spin up a mock local HTTP server that responds with 200 for npm
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind listener");
    let port = listener.local_addr().unwrap().port();
    let npm_base_url = format!("http://127.0.0.1:{port}");

    thread::spawn(move || {
        for stream in listener.incoming() {
            if let Ok(mut stream) = stream {
                let mut buffer = [0; 1024];
                let _ = stream.read(&mut buffer);
                let response = "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{\"version\":\"2.2.2\"}";
                let _ = stream.write_all(response.as_bytes());
                let _ = stream.flush();
                break;
            }
        }
    });

    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_millis(500))
        .build()
        .unwrap();

    // ACP registry pointing to an unbindable port (fails immediately), npm succeeds
    let result = UpstreamProbe::probe_agent_with_urls(
        &client,
        Some("non-existent-registry-id"),
        Some("@agentclientprotocol/codex-acp"),
        "http://127.0.0.1:1", // immediate refusal
        &npm_base_url,
        Duration::from_millis(500),
    );

    assert!(result.is_some());
    let info = result.unwrap();
    assert_eq!(info.version, "2.2.2");
    assert_eq!(info.source, UpstreamProbeSource::Npm);
}

#[test]
fn test_probe_acp_registry_extracts_binary_targets_and_finds_match() {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind listener");
    let port = listener.local_addr().unwrap().port();
    let registry_url = format!("http://127.0.0.1:{port}/antigravity-acp/agent.json");

    thread::spawn(move || {
        for stream in listener.incoming() {
            if let Ok(mut stream) = stream {
                let mut buffer = [0; 1024];
                let _ = stream.read(&mut buffer);
                let json = r#"{
                    "id": "antigravity-acp",
                    "version": "1.4.0",
                    "distribution": {
                        "binary": {
                            "darwin-aarch64": {
                                "archive": "https://dl.google.com/agy-extensions/releases/macos/agy-acp-server-1.4.0-darwin-arm64.zip",
                                "cmd": "./agy_acp_server.par"
                            },
                            "linux-x86_64": {
                                "archive": "https://dl.google.com/agy-extensions/releases/linux/agy-acp-server-1.4.0-linux-x86_64.zip",
                                "cmd": "./agy_acp_server.par",
                                "args": ["--uid="]
                            }
                        }
                    }
                }"#;
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nConnection: close\r\n\r\n{json}"
                );
                let _ = stream.write_all(response.as_bytes());
                let _ = stream.flush();
                break;
            }
        }
    });

    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_millis(500))
        .build()
        .unwrap();

    let info =
        UpstreamProbe::probe_acp_registry(&client, &registry_url, Duration::from_millis(500))
            .expect("probe should succeed");

    assert_eq!(info.version, "1.4.0");
    assert_eq!(info.source, UpstreamProbeSource::AcpRegistry);

    let darwin_target = info
        .find_binary_target("darwin", "aarch64")
        .expect("darwin target found");
    assert_eq!(
        darwin_target.archive,
        "https://dl.google.com/agy-extensions/releases/macos/agy-acp-server-1.4.0-darwin-arm64.zip"
    );
    assert_eq!(darwin_target.cmd.as_deref(), Some("./agy_acp_server.par"));

    // Also test alias matching (macos -> darwin, arm64 -> aarch64)
    let macos_target = info
        .find_binary_target("macos", "arm64")
        .expect("macos alias target found");
    assert_eq!(macos_target.archive, darwin_target.archive);

    let linux_target = info
        .find_binary_target("linux", "x86_64")
        .expect("linux target found");
    assert_eq!(
        linux_target.args.as_ref().unwrap(),
        &vec!["--uid=".to_string()]
    );

    assert!(info.find_binary_target("windows", "x86_64").is_none());
}
