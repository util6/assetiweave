use crate::backend::{
    dto::AppResult,
    models::{AssetKind, Source, SourceKind, SourceOrigin, SourceScannerKind},
    path_utils::normalize_path_for_storage,
};
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, fs, path::Path, path::PathBuf};
use walkdir::WalkDir;

pub(crate) const SYSTEM_SKILLS_MARKER: &str = ".assetiweave-system-skills.marker";
pub(crate) const SYSTEM_SKILL_SOURCE_ID: &str = "assetiweave-system-skills";

const ORGANIZER_SKILL_DIR: &str = "assetiweave-conversation-organizer";
const ORGANIZER_SKILL: &[u8] =
    include_bytes!("../../../builtin-assets/skills/assetiweave-conversation-organizer/SKILL.md");
const ORGANIZER_MANIFEST: &[u8] = include_bytes!(
    "../../../builtin-assets/skills/assetiweave-conversation-organizer/assetiweave.skill.json"
);
const ORGANIZER_ZCODE_MANIFEST: &[u8] =
    include_bytes!("../../../builtin-assets/adapters/zcode/conversation-adapter.json");
const ORGANIZER_ZCODE_ADAPTER: &[u8] =
    include_bytes!("../../../builtin-assets/adapters/zcode/adapter.mjs");
const RECALL_SKILL: &[u8] =
    include_bytes!("../../../builtin-assets/skills/assetiweave-conversation-recall/SKILL.md");
const RECALL_MANIFEST: &[u8] = include_bytes!(
    "../../../builtin-assets/skills/assetiweave-conversation-recall/assetiweave.skill.json"
);
const WEB_REPAIR_SKILL: &[u8] =
    include_bytes!("../../../builtin-assets/skills/assetiweave-web-conversation-repair/SKILL.md");
const WEB_REPAIR_MANIFEST: &[u8] = include_bytes!(
    "../../../builtin-assets/skills/assetiweave-web-conversation-repair/assetiweave.skill.json"
);
const MEMORY_SKILL: &[u8] =
    include_bytes!("../../../builtin-assets/skills/assetiweave-memory/SKILL.md");
const MEMORY_MANIFEST: &[u8] =
    include_bytes!("../../../builtin-assets/skills/assetiweave-memory/assetiweave.skill.json");
const MEMORY_RECALL_SCRIPT: &[u8] =
    include_bytes!("../../../builtin-assets/skills/assetiweave-memory/scripts/recall.py");

struct EmbeddedFile {
    relative_path: &'static str,
    contents: &'static [u8],
    executable: bool,
}

const EMBEDDED_FILES: &[EmbeddedFile] = &[
    EmbeddedFile {
        relative_path: "assetiweave-conversation-organizer/SKILL.md",
        contents: ORGANIZER_SKILL,
        executable: false,
    },
    EmbeddedFile {
        relative_path: "assetiweave-conversation-organizer/assetiweave.skill.json",
        contents: ORGANIZER_MANIFEST,
        executable: false,
    },
    EmbeddedFile {
        relative_path: "assetiweave-conversation-organizer/scripts/zcode-conversation-adapter/conversation-adapter.json",
        contents: ORGANIZER_ZCODE_MANIFEST,
        executable: false,
    },
    EmbeddedFile {
        relative_path: "assetiweave-conversation-organizer/scripts/zcode-conversation-adapter/adapter.mjs",
        contents: ORGANIZER_ZCODE_ADAPTER,
        executable: true,
    },
    EmbeddedFile {
        relative_path: "assetiweave-conversation-recall/SKILL.md",
        contents: RECALL_SKILL,
        executable: false,
    },
    EmbeddedFile {
        relative_path: "assetiweave-conversation-recall/assetiweave.skill.json",
        contents: RECALL_MANIFEST,
        executable: false,
    },
    EmbeddedFile {
        relative_path: "assetiweave-web-conversation-repair/SKILL.md",
        contents: WEB_REPAIR_SKILL,
        executable: false,
    },
    EmbeddedFile {
        relative_path: "assetiweave-web-conversation-repair/assetiweave.skill.json",
        contents: WEB_REPAIR_MANIFEST,
        executable: false,
    },
    EmbeddedFile {
        relative_path: "assetiweave-memory/SKILL.md",
        contents: MEMORY_SKILL,
        executable: false,
    },
    EmbeddedFile {
        relative_path: "assetiweave-memory/assetiweave.skill.json",
        contents: MEMORY_MANIFEST,
        executable: false,
    },
    EmbeddedFile {
        relative_path: "assetiweave-memory/scripts/recall.py",
        contents: MEMORY_RECALL_SCRIPT,
        executable: true,
    },
];

struct EmbeddedSkill {
    directory: &'static str,
    name: &'static str,
    id: &'static str,
    skill: &'static [u8],
    manifest: &'static [u8],
}

const EMBEDDED_SKILLS: &[EmbeddedSkill] = &[
    EmbeddedSkill {
        directory: ORGANIZER_SKILL_DIR,
        name: "assetiweave-conversation-organizer",
        id: "assetiweave.conversation-organizer",
        skill: ORGANIZER_SKILL,
        manifest: ORGANIZER_MANIFEST,
    },
    EmbeddedSkill {
        directory: "assetiweave-conversation-recall",
        name: "assetiweave-conversation-recall",
        id: "assetiweave.conversation-recall",
        skill: RECALL_SKILL,
        manifest: RECALL_MANIFEST,
    },
    EmbeddedSkill {
        directory: "assetiweave-web-conversation-repair",
        name: "assetiweave-web-conversation-repair",
        id: "assetiweave.web-conversation-repair",
        skill: WEB_REPAIR_SKILL,
        manifest: WEB_REPAIR_MANIFEST,
    },
    EmbeddedSkill {
        directory: "assetiweave-memory",
        name: "assetiweave-memory",
        id: "assetiweave.memory",
        skill: MEMORY_SKILL,
        manifest: MEMORY_MANIFEST,
    },
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BuiltinSkillInstallResult {
    pub(crate) root: PathBuf,
    pub(crate) fingerprint: String,
    pub(crate) changed: bool,
}

pub(crate) fn system_skill_root() -> AppResult<PathBuf> {
    let home = dirs::home_dir().ok_or("无法确定用户主目录")?;
    Ok(home.join(".assetiweave").join("skills").join(".system"))
}

pub(crate) fn install_builtin_skills() -> AppResult<BuiltinSkillInstallResult> {
    install_builtin_skills_at(&system_skill_root()?)
}

pub(crate) fn system_skill_source() -> AppResult<Source> {
    let root = system_skill_root()?;
    let root_path = normalize_path_for_storage(&root.to_string_lossy())?;
    Ok(Source {
        id: SYSTEM_SKILL_SOURCE_ID.to_string(),
        name: "AssetIWeave System Skills".to_string(),
        kind: SourceKind::Local,
        root_path,
        scanner_kind: SourceScannerKind::Skill,
        source_origin: SourceOrigin::AssetiweaveSystem,
        repo_root: None,
        scan_root: String::new(),
        origin_app_kind: None,
        origin_provider_id: None,
        include_globs: vec!["**/SKILL.md".to_string()],
        exclude_globs: vec![
            "**/.git/**".to_string(),
            "**/node_modules/**".to_string(),
            "**/target/**".to_string(),
            "**/dist/**".to_string(),
        ],
        default_kind: Some(AssetKind::Skill),
        enabled: true,
        priority: -200,
        last_scanned_at: None,
        last_scan_status: Some("pending".to_string()),
    })
}

fn install_builtin_skills_at(root: &Path) -> AppResult<BuiltinSkillInstallResult> {
    validate_embedded_skills()?;
    let fingerprint = embedded_fingerprint();
    if installed_tree_matches(root, &fingerprint)? {
        return Ok(BuiltinSkillInstallResult {
            root: root.to_path_buf(),
            fingerprint,
            changed: false,
        });
    }

    let parent = root
        .parent()
        .ok_or_else(|| format!("system Skill root has no parent: {}", root.display()))?;
    fs::create_dir_all(parent).map_err(|error| {
        format!(
            "create system Skill parent directory {}: {error}",
            parent.display()
        )
    })?;
    let suffix = uuid::Uuid::new_v4();
    let staging = parent.join(format!(".system.install-{suffix}"));
    let previous = parent.join(format!(".system.previous-{suffix}"));
    write_embedded_tree(&staging, &fingerprint)?;

    if path_is_present(root) {
        fs::rename(root, &previous).map_err(|error| {
            format!(
                "stage existing system Skills {} for replacement: {error}",
                root.display()
            )
        })?;
    }
    if let Err(error) = fs::rename(&staging, root) {
        if path_is_present(&previous) {
            let _ = fs::rename(&previous, root);
        }
        let _ = remove_path(&staging);
        return Err(format!(
            "activate system Skills {}: {error}",
            root.display()
        ));
    }
    if path_is_present(&previous) {
        if let Err(error) = remove_path(&previous) {
            eprintln!(
                "failed to remove previous AssetIWeave system Skills {}: {error}",
                previous.display()
            );
        }
    }

    Ok(BuiltinSkillInstallResult {
        root: root.to_path_buf(),
        fingerprint,
        changed: true,
    })
}

fn validate_embedded_skills() -> AppResult<()> {
    for embedded in EMBEDDED_SKILLS {
        let skill = std::str::from_utf8(embedded.skill)
            .map_err(|error| format!("decode {}/SKILL.md: {error}", embedded.directory))?;
        if !skill.starts_with("---\n")
            || !skill.contains(&format!("name: {}", embedded.name))
            || !skill.contains("description:")
        {
            return Err(format!(
                "embedded Skill {} has invalid frontmatter",
                embedded.directory
            ));
        }
        let manifest: serde_json::Value = serde_json::from_slice(embedded.manifest)
            .map_err(|error| format!("decode {} manifest: {error}", embedded.directory))?;
        if manifest.get("id").and_then(serde_json::Value::as_str) != Some(embedded.id)
            || manifest.get("entry").and_then(serde_json::Value::as_str) != Some("SKILL.md")
            || manifest
                .get("engine_contract")
                .and_then(serde_json::Value::as_str)
                != Some(">=3")
        {
            return Err(format!(
                "embedded Skill {} has an invalid manifest contract",
                embedded.directory
            ));
        }
    }
    Ok(())
}

fn embedded_fingerprint() -> String {
    let mut digest = Sha256::new();
    for file in EMBEDDED_FILES {
        digest.update(file.relative_path.as_bytes());
        digest.update([0]);
        digest.update(file.contents.len().to_le_bytes());
        digest.update(file.contents);
        digest.update([u8::from(file.executable)]);
    }
    format!("{:x}", digest.finalize())
}

fn installed_tree_matches(root: &Path, fingerprint: &str) -> AppResult<bool> {
    if !root.is_dir() {
        return Ok(false);
    }
    let marker = match fs::read_to_string(root.join(SYSTEM_SKILLS_MARKER)) {
        Ok(marker) => marker,
        Err(_) => return Ok(false),
    };
    if marker.trim() != fingerprint {
        return Ok(false);
    }

    let expected_paths = EMBEDDED_FILES
        .iter()
        .map(|file| file.relative_path.to_string())
        .collect::<BTreeSet<_>>();
    let mut installed_paths = BTreeSet::new();
    for entry in WalkDir::new(root).follow_links(false) {
        let entry = entry.map_err(|error| error.to_string())?;
        if entry.depth() > 0 && entry.file_type().is_symlink() {
            return Ok(false);
        }
        if !entry.file_type().is_file() {
            continue;
        }
        let relative = entry
            .path()
            .strip_prefix(root)
            .map_err(|error| error.to_string())?;
        let relative = relative.to_string_lossy().replace('\\', "/");
        if relative == SYSTEM_SKILLS_MARKER {
            continue;
        }
        installed_paths.insert(relative);
    }
    if installed_paths != expected_paths {
        return Ok(false);
    }
    for file in EMBEDDED_FILES {
        let path = root.join(file.relative_path);
        if fs::read(&path).map_err(|error| error.to_string())? != file.contents
            || !executable_mode_matches(&path, file.executable)?
        {
            return Ok(false);
        }
    }
    Ok(true)
}

fn write_embedded_tree(root: &Path, fingerprint: &str) -> AppResult<()> {
    if path_is_present(root) {
        remove_path(root)?;
    }
    for file in EMBEDDED_FILES {
        let path = root.join(file.relative_path);
        let parent = path
            .parent()
            .ok_or_else(|| format!("embedded file has no parent: {}", path.display()))?;
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        fs::write(&path, file.contents).map_err(|error| error.to_string())?;
        set_executable_if_needed(&path, file.executable)?;
    }
    fs::write(root.join(SYSTEM_SKILLS_MARKER), format!("{fingerprint}\n"))
        .map_err(|error| error.to_string())
}

fn path_is_present(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok()
}

fn remove_path(path: &Path) -> AppResult<()> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.to_string()),
    };
    if metadata.file_type().is_dir() && !metadata.file_type().is_symlink() {
        fs::remove_dir_all(path).map_err(|error| error.to_string())
    } else {
        fs::remove_file(path).map_err(|error| error.to_string())
    }
}

#[cfg(unix)]
fn set_executable_if_needed(path: &Path, executable: bool) -> AppResult<()> {
    use std::os::unix::fs::PermissionsExt;
    if executable {
        let mut permissions = fs::metadata(path)
            .map_err(|error| error.to_string())?
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(path, permissions).map_err(|error| error.to_string())?;
    }
    Ok(())
}

#[cfg(unix)]
fn executable_mode_matches(path: &Path, expected: bool) -> AppResult<bool> {
    use std::os::unix::fs::PermissionsExt;
    let executable = fs::metadata(path)
        .map_err(|error| error.to_string())?
        .permissions()
        .mode()
        & 0o111
        != 0;
    Ok(executable == expected)
}

#[cfg(not(unix))]
fn set_executable_if_needed(_path: &Path, _executable: bool) -> AppResult<()> {
    Ok(())
}

#[cfg(not(unix))]
fn executable_mode_matches(_path: &Path, _expected: bool) -> AppResult<bool> {
    Ok(true)
}

#[cfg(test)]
mod tests {
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
    fn packaged_zcode_adapter_matches_the_builtin_asset_source() {
        assert_eq!(
            ORGANIZER_ZCODE_MANIFEST,
            include_bytes!("../../../builtin-assets/adapters/zcode/conversation-adapter.json")
        );
        assert_eq!(
            ORGANIZER_ZCODE_ADAPTER,
            include_bytes!("../../../builtin-assets/adapters/zcode/adapter.mjs")
        );
    }

    fn unique_temp_dir(prefix: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("{prefix}-{}", uuid::Uuid::new_v4()))
    }
}
