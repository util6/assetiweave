use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;

pub struct HarvesterOperations;

#[derive(Debug, Serialize, Deserialize)]
pub struct TemplateSummary {
    pub id: String,
    pub name: String,
    pub version: String,
    pub origin: String,
    pub channel: String,
    pub installed: bool,
    pub directory: String,
    pub description: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct InstallResult {
    pub id: String,
    pub directory: String,
    pub version: String,
    pub action: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct RunResult {
    pub id: String,
    pub program: String,
    pub exit_code: i32,
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
    pub duration_ms: u128,
}

impl HarvesterOperations {
    pub fn default_root() -> PathBuf {
        if let Ok(root) = env::var("ASSETIWEAVE_HARVESTER_ROOT") {
            if !root.trim().is_empty() {
                return PathBuf::from(root.trim());
            }
        }
        let home = env::var("HOME")
            .or_else(|_| env::var("USERPROFILE"))
            .unwrap_or_else(|_| ".".to_string());
        PathBuf::from(home).join(".assetiweave").join("harvesters")
    }

    pub fn resolve_root(custom: Option<String>) -> PathBuf {
        match custom {
            Some(r) if !r.trim().is_empty() => PathBuf::from(r.trim()),
            _ => Self::default_root(),
        }
    }

    fn template_source_dir() -> PathBuf {
        // 优先查看同级或编译时指定的 templates 目录
        let exe_dir = env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.to_path_buf()))
            .unwrap_or_else(|| PathBuf::from("."));

        let candidate1 = exe_dir.join("templates");
        if candidate1.is_dir() {
            return candidate1;
        }

        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("templates");
        if manifest_dir.is_dir() {
            return manifest_dir;
        }

        // fallback to cli/internal/harvesters/templates
        let root_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .map(|p| p.join("cli/internal/harvesters/templates"))
            .unwrap_or_else(|| PathBuf::from("templates"));
        root_dir
    }

    pub fn list_templates(root_override: Option<String>) -> Result<Vec<TemplateSummary>> {
        let root = Self::resolve_root(root_override);
        let src_dir = Self::template_source_dir();

        let mut list = Vec::new();
        if !src_dir.exists() {
            return Ok(list);
        }

        let entries = fs::read_dir(&src_dir)
            .with_context(|| format!("failed to read templates from {:?}", src_dir))?;

        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let manifest_path = path.join("harvester.json");
                if manifest_path.exists() {
                    if let Ok(content) = fs::read_to_string(&manifest_path) {
                        if let Ok(manifest) = serde_json::from_str::<Value>(&content) {
                            let id = manifest
                                .get("id")
                                .and_then(Value::as_str)
                                .unwrap_or("")
                                .to_string();
                            let name = manifest
                                .get("name")
                                .and_then(Value::as_str)
                                .unwrap_or("")
                                .to_string();
                            let version = manifest
                                .get("version")
                                .and_then(Value::as_str)
                                .unwrap_or("")
                                .to_string();
                            let origin = manifest
                                .get("origin")
                                .and_then(Value::as_str)
                                .unwrap_or("official")
                                .to_string();
                            let description = manifest
                                .get("description")
                                .and_then(Value::as_str)
                                .unwrap_or("")
                                .to_string();
                            let channel = manifest
                                .get("update")
                                .and_then(|u| u.get("channel"))
                                .and_then(Value::as_str)
                                .unwrap_or("stable")
                                .to_string();

                            let target_dir = root.join(&id);
                            let installed = target_dir.join("harvester.json").exists();

                            list.push(TemplateSummary {
                                id,
                                name,
                                version,
                                origin,
                                channel,
                                installed,
                                directory: target_dir.to_string_lossy().to_string(),
                                description,
                            });
                        }
                    }
                }
            }
        }

        list.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(list)
    }

    pub fn list_installed(root_override: Option<String>) -> Result<Vec<TemplateSummary>> {
        let root = Self::resolve_root(root_override);
        let mut list = Vec::new();
        if !root.exists() {
            return Ok(list);
        }

        let entries = fs::read_dir(&root)
            .with_context(|| format!("failed to read installed harvesters from {:?}", root))?;

        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let manifest_path = path.join("harvester.json");
                if manifest_path.exists() {
                    if let Ok(content) = fs::read_to_string(&manifest_path) {
                        if let Ok(manifest) = serde_json::from_str::<Value>(&content) {
                            let id = manifest
                                .get("id")
                                .and_then(Value::as_str)
                                .unwrap_or("")
                                .to_string();
                            let name = manifest
                                .get("name")
                                .and_then(Value::as_str)
                                .unwrap_or("")
                                .to_string();
                            let version = manifest
                                .get("version")
                                .and_then(Value::as_str)
                                .unwrap_or("")
                                .to_string();
                            let origin = manifest
                                .get("origin")
                                .and_then(Value::as_str)
                                .unwrap_or("installed")
                                .to_string();
                            let description = manifest
                                .get("description")
                                .and_then(Value::as_str)
                                .unwrap_or("")
                                .to_string();
                            let channel = manifest
                                .get("update")
                                .and_then(|u| u.get("channel"))
                                .and_then(Value::as_str)
                                .unwrap_or("stable")
                                .to_string();

                            list.push(TemplateSummary {
                                id,
                                name,
                                version,
                                origin,
                                channel,
                                installed: true,
                                directory: path.to_string_lossy().to_string(),
                                description,
                            });
                        }
                    }
                }
            }
        }

        list.sort_by(|a, b| a.id.cmp(&b.id));
        Ok(list)
    }

    pub fn install(
        id: &str,
        from: Option<String>,
        root_override: Option<String>,
        force: bool,
    ) -> Result<InstallResult> {
        let root = Self::resolve_root(root_override);
        let target_dir = root.join(id);

        if target_dir.exists() && !force {
            bail!(
                "harvester '{}' already exists at {:?}. Use --force to replace it.",
                id,
                target_dir
            );
        }

        let src_dir = if let Some(from_path) = from {
            PathBuf::from(from_path)
        } else {
            Self::template_source_dir().join(id)
        };

        if !src_dir.exists() {
            bail!("template or source directory not found: {:?}", src_dir);
        }

        fs::create_dir_all(&target_dir)
            .with_context(|| format!("failed to create harvester directory {:?}", target_dir))?;

        // 递归拷贝
        copy_dir_recursive(&src_dir, &target_dir)?;

        let manifest_path = target_dir.join("harvester.json");
        let version = if manifest_path.exists() {
            let content = fs::read_to_string(&manifest_path).unwrap_or_default();
            serde_json::from_str::<Value>(&content)
                .ok()
                .and_then(|v| v.get("version").and_then(Value::as_str).map(str::to_string))
                .unwrap_or_else(|| "1.0.0".to_string())
        } else {
            "1.0.0".to_string()
        };

        Ok(InstallResult {
            id: id.to_string(),
            directory: target_dir.to_string_lossy().to_string(),
            version,
            action: if force {
                "reinstalled".to_string()
            } else {
                "installed".to_string()
            },
        })
    }

    pub fn update(
        id: Option<String>,
        from: Option<String>,
        root_override: Option<String>,
        all: bool,
    ) -> Result<Vec<InstallResult>> {
        let mut results = Vec::new();
        if all {
            let templates = Self::list_templates(root_override.clone())?;
            for t in templates {
                if t.installed {
                    let res = Self::install(&t.id, from.clone(), root_override.clone(), true)?;
                    results.push(res);
                }
            }
        } else if let Some(target_id) = id {
            let res = Self::install(&target_id, from, root_override, true)?;
            results.push(res);
        } else {
            bail!("either template ID or --all must be specified");
        }
        Ok(results)
    }

    pub fn doctor(id: &str, root_override: Option<String>) -> Result<Value> {
        let root = Self::resolve_root(root_override);
        let target_dir = root.join(id);
        if !target_dir.exists() {
            bail!("harvester '{}' is not installed in {:?}", id, root);
        }

        let manifest_path = target_dir.join("harvester.json");
        if !manifest_path.exists() {
            bail!("harvester.json missing in {:?}", target_dir);
        }

        let content = fs::read_to_string(&manifest_path)?;
        let manifest: Value = serde_json::from_str(&content)?;

        let mut checks = Vec::new();

        // 检查 node
        let node_check = Command::new("node").arg("--version").output();
        match node_check {
            Ok(out) if out.status.success() => {
                let v = String::from_utf8_lossy(&out.stdout).trim().to_string();
                checks.push(json!({
                    "name": "runtime:node",
                    "status": "pass",
                    "message": format!("node {}", v)
                }));
            }
            _ => {
                checks.push(json!({
                    "name": "runtime:node",
                    "status": "fail",
                    "message": "node not found in PATH"
                }));
            }
        }

        // 检查 entrypoint
        let entry_ok = target_dir.join("adapter.js").exists()
            || target_dir.join("scripts/harvest.js").exists();
        checks.push(json!({
            "name": "entrypoint",
            "status": if entry_ok { "pass" } else { "warn" },
            "message": format!("directory: {}", target_dir.to_string_lossy())
        }));

        Ok(json!({
            "id": id,
            "manifest": manifest,
            "checks": checks
        }))
    }

    pub fn repair(
        id: &str,
        root_override: Option<String>,
        dry_run: bool,
        yes: bool,
    ) -> Result<Value> {
        if !dry_run && !yes {
            bail!("repairing harvester requires --yes confirmation");
        }
        let root = Self::resolve_root(root_override.clone());
        let target_dir = root.join(id);
        if !target_dir.exists() {
            bail!("harvester '{}' not found", id);
        }
        if dry_run {
            return Ok(json!({
                "id": id,
                "action": "repair_preview",
                "target_dir": target_dir.to_string_lossy()
            }));
        }
        let res = Self::install(id, None, root_override, true)?;
        Ok(serde_json::to_value(res)?)
    }

    pub fn run(
        id: &str,
        args: Vec<String>,
        root_override: Option<String>,
        _timeout_seconds: u64,
        yes: bool,
    ) -> Result<RunResult> {
        if !yes {
            bail!("running harvester script requires --yes confirmation");
        }

        let root = Self::resolve_root(root_override);
        let target_dir = root.join(id);
        if !target_dir.exists() {
            bail!("harvester '{}' is not installed", id);
        }

        let start = Instant::now();

        // 优先检查 scripts/harvest.js 或 adapter.js
        let script = if target_dir.join("scripts/harvest.js").exists() {
            target_dir.join("scripts/harvest.js")
        } else if target_dir.join("adapter.js").exists() {
            target_dir.join("adapter.js")
        } else {
            bail!("no executable entrypoint found in {:?}", target_dir);
        };

        let mut cmd = Command::new("node");
        cmd.arg(&script)
            .args(&args)
            .current_dir(&target_dir)
            .env("ASSETIWEAVE_HARVESTER_DIR", &target_dir);

        let output = cmd
            .output()
            .with_context(|| format!("failed to execute harvester script {:?}", script))?;

        let duration_ms = start.elapsed().as_millis();
        let exit_code = output.status.code().unwrap_or(-1);

        Ok(RunResult {
            id: id.to_string(),
            program: "node".to_string(),
            exit_code,
            success: output.status.success(),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            duration_ms,
        })
    }
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)?.flatten() {
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(())
}
