use std::{fs, path::PathBuf, process::Command};

use crate::backend::host_process::resolve_host_executable;

use super::{
    ensure_staging_root, is_cancelled, run_host_command, runtime_from_local, InstallContext,
    InstallError, Installer,
};
use crate::backend::agent_market::types::{Distribution, MaterializedRuntime};

#[derive(Clone, Debug, Default)]
pub(crate) struct NpxInstaller {
    pub(crate) npm_path: Option<PathBuf>,
}

impl NpxInstaller {
    pub(crate) fn npm_args(
        package: &str,
        version: &str,
        staging_dir: &std::path::Path,
    ) -> Vec<String> {
        vec![
            "install".to_string(),
            "--prefix".to_string(),
            staging_dir.to_string_lossy().to_string(),
            "--save-exact".to_string(),
            "--omit=dev".to_string(),
            "--ignore-scripts".to_string(),
            "--no-audit".to_string(),
            "--no-fund".to_string(),
            format!("{package}@{version}"),
        ]
    }
}

impl Installer for NpxInstaller {
    fn materialize(
        &self,
        distribution: &Distribution,
        context: &InstallContext,
    ) -> Result<MaterializedRuntime, InstallError> {
        let Distribution::Npx {
            package,
            version,
            bin,
            launch_args,
            ..
        } = distribution
        else {
            return Err(InstallError::Unsupported(
                "npx installer received a non-npx distribution".to_string(),
            ));
        };
        let npm = self
            .npm_path
            .clone()
            .or_else(|| resolve_host_executable("npm"))
            .ok_or_else(|| {
                InstallError::RuntimeMissing("npm is required for Npx installation".to_string())
            })?;
        if is_cancelled(context) {
            return Err(InstallError::Cancelled);
        }
        ensure_staging_root(&context.staging_dir)?;
        let npm_config_dir = context.staging_dir.join("npm-config");
        fs::create_dir_all(&npm_config_dir)
            .map_err(|error| InstallError::Failed(error.to_string()))?;
        let user_config = npm_config_dir.join(".npmrc");
        fs::write(
            &user_config,
            b"ignore-scripts=true\naudit=false\nfund=false\n",
        )
        .map_err(|error| InstallError::Failed(error.to_string()))?;
        let npm_cache = context.staging_dir.join("npm-cache");
        fs::create_dir_all(&npm_cache).map_err(|error| InstallError::Failed(error.to_string()))?;
        let args = Self::npm_args(package, version, &context.staging_dir);
        let mut command = Command::new(npm);
        command
            .args(&args)
            .env("npm_config_userconfig", &user_config)
            .env("npm_config_cache", &npm_cache);
        let output = run_host_command(&mut command, context, 1024 * 1024, 256 * 1024)?;
        if !output.status.success() {
            return Err(InstallError::Failed("npm install failed".to_string()));
        }
        let lock_path = context.staging_dir.join("package-lock.json");
        if !lock_path.is_file() {
            return Err(InstallError::Failed(
                "npm did not create package-lock.json".to_string(),
            ));
        }
        let program = context
            .staging_dir
            .join("node_modules")
            .join(".bin")
            .join(bin);
        if !program.exists() {
            return Err(InstallError::ArchiveInvalid(
                "npm bin is outside the staging installation".to_string(),
            ));
        }
        let lock = serde_json::from_slice::<serde_json::Value>(
            &fs::read(&lock_path).map_err(|error| InstallError::Failed(error.to_string()))?,
        )
        .map_err(|error| InstallError::Failed(format!("package-lock.json is invalid: {error}")))?;
        let package_key = format!("node_modules/{package}");
        let locked_package = lock
            .get("packages")
            .and_then(serde_json::Value::as_object)
            .and_then(|packages| packages.get(&package_key))
            .ok_or_else(|| {
                InstallError::Failed("target package is missing from package-lock.json".to_string())
            })?;
        if locked_package
            .get("version")
            .and_then(serde_json::Value::as_str)
            != Some(version)
        {
            return Err(InstallError::Failed(
                "package-lock.json target version does not match the catalog".to_string(),
            ));
        }
        let lock_integrity = locked_package
            .get("integrity")
            .and_then(serde_json::Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| {
                InstallError::Failed("package-lock.json target integrity is missing".to_string())
            })?;
        let integrity = Some(serde_json::json!({
            "package": package,
            "version": version,
            "integrity": lock_integrity,
        }));
        runtime_from_local(context, program, launch_args.clone(), integrity)
    }
}
