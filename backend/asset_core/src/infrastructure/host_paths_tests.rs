use super::*;
use std::path::{Path, PathBuf};

#[test]
fn normalizes_absolute_home_paths_to_portable_home_storage() {
    let resolver = macos_resolver();

    let stored = resolver
        .normalize_input("/Users/alice/.codex/skills")
        .expect("normalize home path");

    assert_eq!(stored.as_str(), "~/.codex/skills");
    assert_eq!(
        resolver
            .resolve(&stored)
            .expect("resolve home path")
            .as_path(),
        Path::new("/Users/alice/.codex/skills")
    );
    assert_eq!(
        resolver
            .display(&stored)
            .expect("display home path")
            .as_str(),
        "~/.codex/skills"
    );
}

#[test]
fn windows_home_paths_are_compared_case_insensitively_and_use_forward_slashes_in_storage() {
    let resolver = windows_resolver();

    let stored = resolver
        .normalize_input(r"c:\USERS\ALICE\.codex\skills")
        .expect("normalize Windows home path");

    assert_eq!(stored.as_str(), "~/.codex/skills");
}

#[test]
fn windows_appdata_alias_resolves_through_config_anchor_but_displays_under_home() {
    let resolver = windows_resolver();

    let stored = resolver
        .normalize_input(r"%APPDATA%\Cursor\skills")
        .expect("normalize APPDATA path");

    assert_eq!(stored.as_str(), "@config/Cursor/skills");
    assert_eq!(
        resolver
            .resolve(&stored)
            .expect("resolve config path")
            .as_path(),
        Path::new(r"C:\Users\Alice\AppData\Roaming\Cursor\skills")
    );
    assert_eq!(
        resolver
            .display(&stored)
            .expect("display config path")
            .as_str(),
        "~/AppData/Roaming/Cursor/skills"
    );
}

#[test]
fn absolute_platform_config_paths_normalize_to_config_anchor_before_home() {
    let macos = macos_resolver();
    let windows = windows_resolver();

    assert_eq!(
        macos
            .normalize_input(
                "/Users/alice/Library/Application Support/assetiweave/conversation-adapters"
            )
            .expect("normalize macOS config path")
            .as_str(),
        "@config/assetiweave/conversation-adapters"
    );
    assert_eq!(
        windows
            .normalize_input(r"C:\Users\Alice\AppData\Roaming\assetiweave\conversation-adapters")
            .expect("normalize Windows config path")
            .as_str(),
        "@config/assetiweave/conversation-adapters"
    );
}

#[test]
fn absolute_paths_outside_home_remain_absolute() {
    let resolver = windows_resolver();

    let stored = resolver
        .normalize_input(r"D:\Shared\skills")
        .expect("normalize external path");

    assert_eq!(stored.as_str(), "D:/Shared/skills");
}

#[test]
fn relative_paths_resolve_from_workspace_and_remain_relative_in_storage() {
    let resolver = macos_resolver();

    let stored = resolver
        .normalize_input("agent-docs/feature-plans/runtime-extension-refactor/00-overview.md")
        .expect("normalize workspace path");

    assert_eq!(
        stored.as_str(),
        "agent-docs/feature-plans/runtime-extension-refactor/00-overview.md"
    );
    assert_eq!(
            resolver
                .resolve(&stored)
                .expect("resolve workspace path")
                .as_path(),
            Path::new("/workspace/assetiweave/agent-docs/feature-plans/runtime-extension-refactor/00-overview.md")
        );
}

#[test]
fn linux_all_anchors_normalize_resolve_and_display_round_trip() {
    let resolver = linux_resolver();

    // Config
    let config_stored = resolver
        .normalize_input("/home/alice/.config/assetiweave/skills")
        .expect("normalize linux config path");
    assert_eq!(config_stored.as_str(), "@config/assetiweave/skills");
    assert_eq!(
        resolver.resolve(&config_stored).expect("resolve").as_path(),
        Path::new("/home/alice/.config/assetiweave/skills")
    );
    assert_eq!(
        resolver.display(&config_stored).expect("display").as_str(),
        "~/.config/assetiweave/skills"
    );

    // Data / LocalData
    let data_stored = resolver
        .normalize_input("/home/alice/.local/share/assetiweave/data")
        .expect("normalize linux data path");
    assert_eq!(data_stored.as_str(), "@local-data/assetiweave/data");
    assert_eq!(
        resolver.resolve(&data_stored).expect("resolve").as_path(),
        Path::new("/home/alice/.local/share/assetiweave/data")
    );

    // Cache
    let cache_stored = resolver
        .normalize_input("/home/alice/.cache/assetiweave/cache")
        .expect("normalize linux cache path");
    assert_eq!(cache_stored.as_str(), "@cache/assetiweave/cache");
    assert_eq!(
        resolver.resolve(&cache_stored).expect("resolve").as_path(),
        Path::new("/home/alice/.cache/assetiweave/cache")
    );

    // Home
    let home_stored = resolver
        .normalize_input("/home/alice/projects/demo")
        .expect("normalize linux home path");
    assert_eq!(home_stored.as_str(), "~/projects/demo");
    assert_eq!(
        resolver.resolve(&home_stored).expect("resolve").as_path(),
        Path::new("/home/alice/projects/demo")
    );
    assert_eq!(
        resolver.display(&home_stored).expect("display").as_str(),
        "~/projects/demo"
    );
}

#[test]
fn longest_prefix_prefers_specific_anchor_over_home() {
    let resolver = macos_resolver();

    let stored = resolver
        .normalize_input("/Users/alice/Library/Caches/assetiweave/logs")
        .expect("normalize cache path");
    // Should match @cache rather than ~
    assert_eq!(stored.as_str(), "@cache/assetiweave/logs");
}

#[test]
fn relative_traversal_paths_are_kept_portable() {
    let resolver = macos_resolver();

    let stored = resolver
        .normalize_input("src/../docs/guide.md")
        .expect("normalize relative traversal path");
    assert_eq!(stored.as_str(), "src/../docs/guide.md");
}

fn macos_resolver() -> HostPathResolver {
    HostPathResolver::new(
        HostPlatform::Macos,
        HostDirectories {
            home: PathBuf::from("/Users/alice"),
            config: PathBuf::from("/Users/alice/Library/Application Support"),
            local_data: PathBuf::from("/Users/alice/Library/Application Support"),
            data: PathBuf::from("/Users/alice/Library/Application Support"),
            cache: PathBuf::from("/Users/alice/Library/Caches"),
            workspace: PathBuf::from("/workspace/assetiweave"),
        },
    )
}

fn windows_resolver() -> HostPathResolver {
    HostPathResolver::new(
        HostPlatform::Windows,
        HostDirectories {
            home: PathBuf::from(r"C:\Users\Alice"),
            config: PathBuf::from(r"C:\Users\Alice\AppData\Roaming"),
            local_data: PathBuf::from(r"C:\Users\Alice\AppData\Local"),
            data: PathBuf::from(r"C:\Users\Alice\AppData\Roaming"),
            cache: PathBuf::from(r"C:\Users\Alice\AppData\Local\Cache"),
            workspace: PathBuf::from(r"C:\workspace\assetiweave"),
        },
    )
}

fn linux_resolver() -> HostPathResolver {
    HostPathResolver::new(
        HostPlatform::Linux,
        HostDirectories {
            home: PathBuf::from("/home/alice"),
            config: PathBuf::from("/home/alice/.config"),
            local_data: PathBuf::from("/home/alice/.local/share"),
            data: PathBuf::from("/home/alice/.local/share"),
            cache: PathBuf::from("/home/alice/.cache"),
            workspace: PathBuf::from("/workspace/assetiweave"),
        },
    )
}
