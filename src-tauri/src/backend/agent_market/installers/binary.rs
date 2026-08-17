use std::{
    collections::HashSet,
    fs::{self, File},
    io::{Cursor, Read},
    path::PathBuf,
};

use sha2::{Digest, Sha256};
use zip::ZipArchive;

use super::{
    ensure_inside, ensure_staging_root, is_cancelled, runtime_from_local, InstallContext,
    InstallError, Installer, MAX_BINARY_BYTES, MAX_FILE_COUNT, MAX_UNPACKED_BYTES,
};
use crate::backend::agent_market::types::{Distribution, MaterializedRuntime};

#[derive(Clone, Debug, Default)]
pub(crate) struct BinaryInstaller;

impl BinaryInstaller {
    pub(crate) fn materialize_bytes(
        &self,
        distribution: &Distribution,
        context: &InstallContext,
        bytes: &[u8],
    ) -> Result<MaterializedRuntime, InstallError> {
        let Distribution::Binary {
            archive,
            sha256,
            executable,
            launch_args,
            ..
        } = distribution
        else {
            return Err(InstallError::Unsupported(
                "binary installer received a non-binary distribution".to_string(),
            ));
        };
        if bytes.len() as u64 > MAX_BINARY_BYTES {
            return Err(InstallError::ArchiveInvalid(
                "binary artifact exceeds size limit".to_string(),
            ));
        }
        if is_cancelled(context) {
            return Err(InstallError::Cancelled);
        }
        let digest = Sha256::digest(bytes);
        if hex_lower(&digest) != *sha256 {
            return Err(InstallError::IntegrityMismatch);
        }
        ensure_staging_root(&context.staging_dir)?;
        match archive.as_str() {
            "none" => {
                let path = context.staging_dir.join(executable);
                ensure_inside(&context.staging_dir, &path)?;
                if let Some(parent) = path.parent() {
                    fs::create_dir_all(parent)
                        .map_err(|error| InstallError::Failed(error.to_string()))?;
                }
                fs::write(&path, bytes).map_err(|error| InstallError::Failed(error.to_string()))?;
                make_executable(&path)?;
            }
            "zip" => extract_zip(&context.staging_dir, bytes, context)?,
            "tar.gz" | "tgz" => extract_tar_gz(&context.staging_dir, bytes, context)?,
            "tar.bz2" | "tbz2" => extract_tar_bz2(&context.staging_dir, bytes, context)?,
            other => {
                return Err(InstallError::Unsupported(format!(
                    "binary archive is not supported in this build: {other}"
                )))
            }
        }
        let program = context.staging_dir.join(executable);
        runtime_from_local(
            context,
            program,
            launch_args.clone(),
            Some(serde_json::json!({ "sha256": sha256, "size": bytes.len() })),
        )
    }
}

impl Installer for BinaryInstaller {
    fn materialize(
        &self,
        distribution: &Distribution,
        _context: &InstallContext,
    ) -> Result<MaterializedRuntime, InstallError> {
        Err(InstallError::Unsupported(format!(
            "binary download requires an injected artifact source for {}",
            distribution.id()
        )))
    }
}

fn extract_zip(
    root: &std::path::Path,
    bytes: &[u8],
    context: &InstallContext,
) -> Result<(), InstallError> {
    let mut archive = ZipArchive::new(Cursor::new(bytes))
        .map_err(|error| InstallError::ArchiveInvalid(error.to_string()))?;
    if archive.len() > MAX_FILE_COUNT {
        return Err(InstallError::ArchiveInvalid(
            "archive contains too many files".to_string(),
        ));
    }
    let mut paths = HashSet::new();
    let mut unpacked = 0_u64;
    for index in 0..archive.len() {
        if is_cancelled(context) {
            return Err(InstallError::Cancelled);
        }
        let mut entry = archive
            .by_index(index)
            .map_err(|error| InstallError::ArchiveInvalid(error.to_string()))?;
        let Some(name) = entry.enclosed_name().map(PathBuf::from) else {
            return Err(InstallError::ArchiveInvalid(
                "archive path escapes staging root".to_string(),
            ));
        };
        if !paths.insert(name.clone()) {
            return Err(InstallError::ArchiveInvalid(
                "archive contains duplicate paths".to_string(),
            ));
        }
        let destination = root.join(name);
        ensure_inside(root, &destination)?;
        if entry.is_dir() {
            fs::create_dir_all(&destination)
                .map_err(|error| InstallError::Failed(error.to_string()))?;
            continue;
        }
        if entry
            .unix_mode()
            .is_some_and(|mode| mode & 0o170000 == 0o120000)
        {
            return Err(InstallError::ArchiveInvalid(
                "archive symlink is not allowed".to_string(),
            ));
        }
        if entry
            .unix_mode()
            .is_some_and(|mode| mode & 0o170000 != 0 && mode & 0o170000 != 0o100000)
        {
            return Err(InstallError::ArchiveInvalid(
                "archive special file is not allowed".to_string(),
            ));
        }
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).map_err(|error| InstallError::Failed(error.to_string()))?;
        }
        let mut bytes_written = 0_u64;
        let mut output =
            File::create(&destination).map_err(|error| InstallError::Failed(error.to_string()))?;
        let mut buffer = [0_u8; 8192];
        loop {
            let count = entry
                .read(&mut buffer)
                .map_err(|error| InstallError::Failed(error.to_string()))?;
            if count == 0 {
                break;
            }
            bytes_written += count as u64;
            unpacked += count as u64;
            if unpacked > MAX_UNPACKED_BYTES {
                return Err(InstallError::ArchiveInvalid(
                    "archive exceeds unpacked size limit".to_string(),
                ));
            }
            std::io::Write::write_all(&mut output, &buffer[..count])
                .map_err(|error| InstallError::Failed(error.to_string()))?;
        }
        if bytes_written == 0 {
            continue;
        }
        make_executable(&destination)?;
    }
    Ok(())
}

fn extract_tar_gz(
    root: &std::path::Path,
    bytes: &[u8],
    context: &InstallContext,
) -> Result<(), InstallError> {
    let decoder = flate2::read::GzDecoder::new(Cursor::new(bytes));
    extract_tar(root, decoder, context)
}

fn extract_tar_bz2(
    root: &std::path::Path,
    bytes: &[u8],
    context: &InstallContext,
) -> Result<(), InstallError> {
    let decoder = bzip2::read::BzDecoder::new(Cursor::new(bytes));
    extract_tar(root, decoder, context)
}

fn extract_tar<R: Read>(
    root: &std::path::Path,
    reader: R,
    context: &InstallContext,
) -> Result<(), InstallError> {
    let mut archive = tar::Archive::new(reader);
    let mut paths = HashSet::new();
    let mut unpacked = 0_u64;
    let mut file_count = 0_usize;
    for entry_result in archive
        .entries()
        .map_err(|error| InstallError::ArchiveInvalid(error.to_string()))?
    {
        if is_cancelled(context) {
            return Err(InstallError::Cancelled);
        }
        file_count += 1;
        if file_count > MAX_FILE_COUNT {
            return Err(InstallError::ArchiveInvalid(
                "archive contains too many files".to_string(),
            ));
        }
        let mut entry =
            entry_result.map_err(|error| InstallError::ArchiveInvalid(error.to_string()))?;
        let path = entry
            .path()
            .map_err(|error| InstallError::ArchiveInvalid(error.to_string()))?
            .to_path_buf();
        if path.is_absolute()
            || path
                .components()
                .any(|component| matches!(component, std::path::Component::ParentDir))
        {
            return Err(InstallError::ArchiveInvalid(
                "archive path escapes staging root".to_string(),
            ));
        }
        if !paths.insert(path.clone()) {
            return Err(InstallError::ArchiveInvalid(
                "archive contains duplicate paths".to_string(),
            ));
        }
        let entry_type = entry.header().entry_type();
        if entry_type.is_symlink()
            || entry_type.is_hard_link()
            || !entry_type.is_file() && !entry_type.is_dir()
        {
            return Err(InstallError::ArchiveInvalid(
                "archive special or linked file is not allowed".to_string(),
            ));
        }
        let destination = root.join(&path);
        ensure_inside(root, &destination)?;
        if entry_type.is_dir() {
            fs::create_dir_all(&destination)
                .map_err(|error| InstallError::Failed(error.to_string()))?;
            continue;
        }
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).map_err(|error| InstallError::Failed(error.to_string()))?;
        }
        let mut output =
            File::create(&destination).map_err(|error| InstallError::Failed(error.to_string()))?;
        let mut buffer = [0_u8; 8192];
        loop {
            let count = entry
                .read(&mut buffer)
                .map_err(|error| InstallError::Failed(error.to_string()))?;
            if count == 0 {
                break;
            }
            unpacked += count as u64;
            if unpacked > MAX_UNPACKED_BYTES {
                return Err(InstallError::ArchiveInvalid(
                    "archive exceeds unpacked size limit".to_string(),
                ));
            }
            std::io::Write::write_all(&mut output, &buffer[..count])
                .map_err(|error| InstallError::Failed(error.to_string()))?;
        }
        make_executable(&destination)?;
    }
    Ok(())
}

fn make_executable(path: &std::path::Path) -> Result<(), InstallError> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let metadata =
            fs::metadata(path).map_err(|error| InstallError::Failed(error.to_string()))?;
        fs::set_permissions(
            path,
            fs::Permissions::from_mode(metadata.permissions().mode() | 0o700),
        )
        .map_err(|error| InstallError::Failed(error.to_string()))?;
    }
    Ok(())
}

fn hex_lower(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
