use super::*;
use std::fs;

#[test]
fn installs_embedded_system_skills_with_a_fingerprint_marker() {
    let root = unique_temp_dir("assetiweave-system-skills-install");

    let result = install_builtin_skills_at(&root).expect("install built-in skills");

    assert!(result.changed);
    assert_eq!(result.root, root);
    assert!(root
        .join("assetiweave-conversation-organizer")
        .join("SKILL.md")
        .is_file());
    assert!(root
        .join("assetiweave-conversation-organizer")
        .join("assetiweave.skill.json")
        .is_file());
    for skill in [
        "assetiweave-conversation-recall",
        "assetiweave-web-conversation-repair",
        "assetiweave-memory",
        "assetiweave-memory-generation",
    ] {
        assert!(root.join(skill).join("SKILL.md").is_file());
        assert!(root.join(skill).join("assetiweave.skill.json").is_file());
    }
    let memory_recall = root
        .join("assetiweave-memory")
        .join("scripts")
        .join("recall.py");
    assert!(memory_recall.is_file());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_ne!(
            fs::metadata(&memory_recall)
                .expect("read Memory Recall script permissions")
                .permissions()
                .mode()
                & 0o111,
            0
        );
    }
    let adapter_dir = root
        .join("assetiweave-conversation-organizer")
        .join("scripts")
        .join("zcode-conversation-adapter");
    assert!(adapter_dir.join("conversation-adapter.json").is_file());
    assert!(adapter_dir.join("adapter.mjs").is_file());
    assert!(adapter_dir.join("shell-projector.cjs").is_file());
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_ne!(
            fs::metadata(adapter_dir.join("adapter.mjs"))
                .expect("read installed adapter permissions")
                .permissions()
                .mode()
                & 0o111,
            0
        );
    }
    assert_eq!(
        fs::read_to_string(root.join(SYSTEM_SKILLS_MARKER))
            .expect("read system Skill marker")
            .trim(),
        result.fingerprint
    );

    fs::remove_dir_all(&root).ok();
}

#[test]
fn skips_an_unchanged_system_skill_installation() {
    let root = unique_temp_dir("assetiweave-system-skills-unchanged");
    install_builtin_skills_at(&root).expect("first install");

    let result = install_builtin_skills_at(&root).expect("second install");

    fs::remove_dir_all(&root).ok();
    assert!(!result.changed);
}

#[test]
fn repairs_tampered_or_unexpected_system_skill_files() {
    let root = unique_temp_dir("assetiweave-system-skills-repair");
    install_builtin_skills_at(&root).expect("first install");
    let skill_file = root
        .join("assetiweave-conversation-organizer")
        .join("SKILL.md");
    fs::write(&skill_file, "tampered").expect("tamper with installed Skill");
    fs::write(root.join("unexpected.txt"), "unexpected").expect("write unexpected file");

    let result = install_builtin_skills_at(&root).expect("repair install");

    assert!(result.changed);
    assert!(fs::read_to_string(skill_file)
        .expect("read repaired Skill")
        .contains("name: assetiweave-conversation-organizer"));
    assert!(!root.join("unexpected.txt").exists());

    fs::remove_dir_all(&root).ok();
}

#[test]
fn replaces_a_non_directory_system_skill_root() {
    let root = unique_temp_dir("assetiweave-system-skills-file-root");
    fs::write(&root, "not a directory").expect("write blocking root file");

    let result = install_builtin_skills_at(&root).expect("replace blocking root file");

    assert!(result.changed);
    assert!(root
        .join("assetiweave-conversation-organizer")
        .join("SKILL.md")
        .is_file());

    fs::remove_dir_all(&root).ok();
}

#[cfg(unix)]
#[test]
fn repairs_executable_permissions_and_unexpected_symlinks() {
    use std::os::unix::fs::{symlink, PermissionsExt};

    let root = unique_temp_dir("assetiweave-system-skills-permissions");
    install_builtin_skills_at(&root).expect("first install");
    let adapter = root
        .join("assetiweave-conversation-organizer")
        .join("scripts")
        .join("zcode-conversation-adapter")
        .join("adapter.mjs");
    let mut permissions = fs::metadata(&adapter)
        .expect("read adapter metadata")
        .permissions();
    permissions.set_mode(0o644);
    fs::set_permissions(&adapter, permissions).expect("remove executable permission");
    symlink(&adapter, root.join("unexpected-link")).expect("create unexpected symlink");

    let result = install_builtin_skills_at(&root).expect("repair install");

    assert!(result.changed);
    assert_ne!(
        fs::metadata(&adapter)
            .expect("read repaired adapter metadata")
            .permissions()
            .mode()
            & 0o111,
        0
    );
    assert!(!root.join("unexpected-link").exists());

    fs::remove_dir_all(&root).ok();
}

#[test]
fn exposes_a_fixed_read_only_system_skill_source() {
    let source = system_skill_source().expect("build system Skill source");

    assert_eq!(source.id, SYSTEM_SKILL_SOURCE_ID);
    assert_eq!(source.source_origin, SourceOrigin::AssetiweaveSystem);
    assert_eq!(source.scanner_kind, SourceScannerKind::Skill);
    assert_eq!(source.root_path, "~/.assetiweave/skills/.system");
    assert!(source.enabled);
    assert_eq!(source.priority, -200);
}

#[test]
fn normalizes_bom_crlf_and_lone_cr_before_validating_frontmatter() {
    let skill =
        "\u{feff}---\r\nname: sample-skill\rdescription: A sample skill.\r\n---\r# Sample Skill";

    assert!(validate_embedded_skill_frontmatter(skill, "sample-skill", "sample-dir").is_ok());
}

#[test]
fn rejects_missing_or_mismatched_embedded_skill_frontmatter() {
    let missing = "name: sample-skill\ndescription: A sample skill.";
    let mismatched = "---\nname: another-skill\ndescription: A sample skill.\n---\n# Sample Skill";

    assert!(validate_embedded_skill_frontmatter(missing, "sample-skill", "sample-dir").is_err());
    assert!(validate_embedded_skill_frontmatter(mismatched, "sample-skill", "sample-dir").is_err());
}

#[test]
fn retries_transient_file_operations() {
    let mut attempts = 0;
    retry_io("test operation", || {
        attempts += 1;
        if attempts < 3 {
            Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied))
        } else {
            Ok(())
        }
    })
    .expect("transient operation should eventually succeed");

    assert_eq!(attempts, 3);
}

#[test]
fn packaged_zcode_adapter_matches_the_builtin_asset_source() {
    assert_eq!(
        ORGANIZER_ZCODE_MANIFEST,
        include_bytes!("../../../builtin-assets/adapters/zcode/conversation-adapter.json")
    );
    assert_eq!(
        ORGANIZER_ZCODE_ADAPTER,
        include_bytes!("../../../builtin-assets/adapters/zcode/adapter.mjs")
    );
    assert_eq!(
        ORGANIZER_ZCODE_SHELL_PROJECTOR,
        include_bytes!("../../../builtin-assets/adapters/zcode/shell-projector.cjs")
    );
}

fn unique_temp_dir(prefix: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("{prefix}-{}", uuid::Uuid::new_v4()))
}
