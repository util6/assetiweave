use semver::Version;
use url::Url;

const MAX_ID_BYTES: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub(crate) enum CatalogValidationError {
    #[error("invalid catalog item id: {0}")]
    InvalidId(String),
    #[error("invalid display name for {0}")]
    InvalidDisplayName(String),
    #[error("invalid description for {0}")]
    InvalidDescription(String),
    #[error("invalid observed version for {0}")]
    InvalidVersion(String),
    #[error("catalog item has no distributions: {0}")]
    NoDistributions(String),
    #[error("duplicate distribution id: {0}")]
    DuplicateDistributionId(String),
    #[error("invalid distribution: {0}")]
    InvalidDistribution(String),
    #[error("invalid session cleanup arguments: {0}")]
    InvalidSessionCleanupArgs(String),
    #[error("invalid session cleanup not-found marker: {0}")]
    InvalidSessionCleanupMarker(String),
    #[error("invalid system distribution: {0}")]
    InvalidSystemDistribution(String),
    #[error("invalid binary integrity metadata: {0}")]
    InvalidBinaryIntegrity(String),
    #[error("invalid binary executable path: {0}")]
    InvalidBinaryExecutable(String),
    #[error("invalid npx distribution: {0}")]
    InvalidNpxDistribution(String),
    #[error("invalid uvx distribution: {0}")]
    InvalidUvxDistribution(String),
    #[error("{id}: {message}")]
    FieldValidation { id: String, message: String },
}

pub(crate) fn is_valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_ID_BYTES
        && value.bytes().enumerate().all(|(index, byte)| {
            (index == 0 && byte.is_ascii_lowercase())
                || (index > 0
                    && (byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-'))
        })
}

pub(crate) fn is_safe_relative_path(value: &str) -> bool {
    let path = std::path::Path::new(value);
    !value.is_empty()
        && !path.is_absolute()
        && !value.contains('\0')
        && path.components().all(|component| {
            !matches!(
                component,
                std::path::Component::ParentDir
                    | std::path::Component::RootDir
                    | std::path::Component::Prefix(_)
            )
        })
}

pub(crate) fn is_valid_npm_package(value: &str) -> bool {
    let mut parts = value.split('/');
    let first = parts.next().unwrap_or_default();
    let name = if first.starts_with('@') {
        let Some(second) = parts.next() else {
            return false;
        };
        if parts.next().is_some() {
            return false;
        }
        format!("{first}/{second}")
    } else {
        if parts.next().is_some() {
            return false;
        }
        first.to_string()
    };
    !name.is_empty() && !name.contains([':', '\\', ' ', '\0']) && !name.ends_with('.')
}

pub(crate) fn is_fixed_version(value: &str) -> bool {
    Version::parse(value).is_ok()
}

pub fn is_safe_artifact_url(value: &str) -> bool {
    let Ok(url) = Url::parse(value) else {
        return false;
    };
    if url.scheme() != "https"
        || url.host_str().is_none()
        || url.username() != ""
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.port().is_some_and(|port| port != 443)
    {
        return false;
    }
    let host = url.host_str().unwrap_or_default().to_ascii_lowercase();
    if matches!(host.as_str(), "localhost" | "localhost.localdomain")
        || host.ends_with(".localhost")
        || host.ends_with(".local")
    {
        return false;
    }
    if host.parse::<std::net::IpAddr>().is_ok() {
        return false;
    }
    true
}

pub(crate) fn is_safe_command_candidate(value: &str) -> bool {
    !value.is_empty()
        && !value.contains(['/', '\\', '\0', ';', '|', '&', '\n', '\r'])
        && value.chars().all(|character| !character.is_whitespace())
}

pub(crate) fn is_valid_python_project(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
}
