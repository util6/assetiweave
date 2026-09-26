use super::io_utils::is_javascript_adapter_command;
use super::prelude::*;

pub(crate) const CONVERSATION_RUNTIME_OVERRIDES_KEY: &str = "conversationRuntimeOverrides";
pub(crate) const LEGACY_JAVASCRIPT_COMMAND_NODE_VERSION: &str = ">=20";

pub(crate) fn adapter_runtime_requirements(
    adapters: &[ConversationAdapter],
) -> Vec<(ConversationAdapterRuntimeKind, String)> {
    let mut requirements: Vec<(ConversationAdapterRuntimeKind, String)> = Vec::new();
    for adapter in adapters {
        if !adapter.enabled {
            continue;
        }
        let Some(manifest_path) = adapter.manifest_path.as_deref() else {
            continue;
        };
        let Ok(manifest_path) =
            crate::backend::infrastructure::path_utils::expand_path(manifest_path)
        else {
            continue;
        };
        let Ok(manifest_text) = fs::read_to_string(manifest_path) else {
            continue;
        };
        let Ok(manifest) = serde_json::from_str::<ConversationAdapterManifest>(&manifest_text)
        else {
            continue;
        };
        if let Some(runtime) = manifest.runtime.as_ref() {
            let Some(version) = runtime.version.as_deref() else {
                continue;
            };
            if matches!(runtime.kind, ConversationAdapterRuntimeKind::Executable)
                || validate_runtime_version_constraint(version).is_err()
            {
                continue;
            }
            upsert_highest_runtime_requirement(&mut requirements, &runtime.kind, version);
        } else if manifest
            .command
            .first()
            .is_some_and(|command| is_javascript_adapter_command(Path::new(command)))
        {
            upsert_highest_runtime_requirement(
                &mut requirements,
                &ConversationAdapterRuntimeKind::Node,
                LEGACY_JAVASCRIPT_COMMAND_NODE_VERSION,
            );
        }
    }
    sort_runtime_requirements(requirements)
}

pub(crate) fn upsert_highest_runtime_requirement(
    requirements: &mut Vec<(ConversationAdapterRuntimeKind, String)>,
    kind: &ConversationAdapterRuntimeKind,
    version: &str,
) {
    if let Some((_, current_version)) = requirements
        .iter_mut()
        .find(|(requirement_kind, _)| requirement_kind == kind)
    {
        if runtime_requirement_is_higher(version, current_version).unwrap_or(false) {
            *current_version = version.to_string();
        }
    } else {
        requirements.push((kind.clone(), version.to_string()));
    }
}

pub(crate) fn runtime_requirement_is_higher(candidate: &str, current: &str) -> InfraResult<bool> {
    let candidate = parse_minimum_runtime_version(candidate)?;
    let current = parse_minimum_runtime_version(current)?;
    Ok(candidate > current)
}

pub(crate) fn sort_runtime_requirements(
    mut requirements: Vec<(ConversationAdapterRuntimeKind, String)>,
) -> Vec<(ConversationAdapterRuntimeKind, String)> {
    let order = [
        ConversationAdapterRuntimeKind::Node,
        ConversationAdapterRuntimeKind::Python,
        ConversationAdapterRuntimeKind::Bash,
    ];
    requirements.sort_by_key(|(kind, _)| {
        order
            .iter()
            .position(|ordered_kind| ordered_kind == kind)
            .unwrap_or(order.len())
    });
    requirements
}

pub(crate) fn runtime_version_mismatch_error(
    kind: &ConversationAdapterRuntimeKind,
    program: &Path,
    requirement: &str,
    detected_version: &str,
) -> String {
    format!(
        "adapter runtime {} requires {requirement}, but {} reported {detected_version}",
        runtime_display_name(kind),
        program.display()
    )
}

pub(crate) fn adapter_runtime_missing_message(
    runtime: &ConversationAdapterRuntime,
    program: &Path,
) -> String {
    let version = runtime
        .version
        .as_deref()
        .map(|version| format!(" {version}"))
        .unwrap_or_default();
    format!(
        "adapter runtime {}{} was not found{}: {}",
        runtime_display_name(&runtime.kind),
        version,
        runtime_program_location_suffix(program),
        program.display()
    )
}

pub(crate) fn validate_runtime_version_constraint(requirement: &str) -> InfraResult<()> {
    parse_minimum_version_constraint(requirement).map(|_| ())
}

pub(crate) fn runtime_version_satisfies_constraint(
    detected_version: &str,
    requirement: &str,
) -> InfraResult<bool> {
    let requirement = parse_minimum_version_constraint(requirement)?;
    let detected = parse_detected_runtime_version(detected_version).ok_or_else(|| {
        InfraError::external({
            format!("could not parse adapter runtime version from output: {detected_version}")
        })
    })?;
    Ok(requirement.matches(&detected))
}

pub(crate) fn parse_minimum_version_constraint(
    requirement: &str,
) -> InfraResult<semver::VersionReq> {
    let minimum = parse_minimum_runtime_version(requirement)?;
    semver::VersionReq::parse(&format!(">={minimum}")).map_err(InfraError::external)
}

pub(crate) fn parse_minimum_runtime_version(requirement: &str) -> InfraResult<semver::Version> {
    let requirement = requirement.trim();
    let version = requirement.strip_prefix(">=").ok_or_else(|| {
        InfraError::external({
            format!("adapter runtime version constraint must use >=x[.y[.z]]: {requirement}")
        })
    })?;
    parse_numeric_runtime_version(version.trim()).ok_or_else(|| {
        InfraError::Validation(format!(
            "adapter runtime version constraint must use >=x[.y[.z]]: {requirement}"
        ))
    })
}

pub(crate) fn parse_numeric_runtime_version(value: &str) -> Option<semver::Version> {
    if value.is_empty() {
        return None;
    }
    let parts = value.split('.').collect::<Vec<_>>();
    if parts.len() > 3 || parts.iter().any(|part| part.is_empty()) {
        return None;
    }
    let mut numbers = Vec::with_capacity(parts.len());
    for part in parts {
        if !part.chars().all(|character| character.is_ascii_digit()) {
            return None;
        }
        let number = part.parse::<u64>().ok()?;
        numbers.push(number);
    }
    let major = *numbers.first().unwrap_or(&0);
    let minor = *numbers.get(1).unwrap_or(&0);
    let patch = *numbers.get(2).unwrap_or(&0);
    Some(semver::Version::new(major, minor, patch))
}

pub(crate) fn parse_detected_runtime_version(output: &str) -> Option<semver::Version> {
    let start = output
        .char_indices()
        .find(|(_, character)| character.is_ascii_digit())
        .map(|(index, _)| index)?;
    let version = output[start..]
        .chars()
        .take_while(|character| character.is_ascii_digit() || *character == '.')
        .collect::<String>();
    parse_numeric_runtime_version(version.trim_end_matches('.'))
}

pub(crate) fn configured_runtime_program(
    kind: &ConversationAdapterRuntimeKind,
    settings: &Value,
) -> PathBuf {
    runtime_program_from_settings(kind, settings).unwrap_or_else(|| default_runtime_program(kind))
}

pub(crate) fn runtime_program_from_settings(
    kind: &ConversationAdapterRuntimeKind,
    settings: &Value,
) -> Option<PathBuf> {
    let overrides = settings
        .get(CONVERSATION_RUNTIME_OVERRIDES_KEY)
        .and_then(Value::as_object)?;
    let key = match kind {
        ConversationAdapterRuntimeKind::Node => "node",
        ConversationAdapterRuntimeKind::Python => "python",
        ConversationAdapterRuntimeKind::Bash => "bash",
        ConversationAdapterRuntimeKind::Executable => return None,
    };
    let program = overrides.get(key)?.as_str()?.trim();
    if program.is_empty() || program.len() > 4096 || !is_absolute_runtime_program(program) {
        return None;
    }
    crate::backend::infrastructure::path_utils::expand_path(program).ok()
}

pub(crate) fn default_runtime_program(kind: &ConversationAdapterRuntimeKind) -> PathBuf {
    match kind {
        ConversationAdapterRuntimeKind::Node => PathBuf::from("node"),
        #[cfg(windows)]
        ConversationAdapterRuntimeKind::Python => PathBuf::from("py"),
        #[cfg(not(windows))]
        ConversationAdapterRuntimeKind::Python => PathBuf::from("python3"),
        ConversationAdapterRuntimeKind::Bash => PathBuf::from("bash"),
        ConversationAdapterRuntimeKind::Executable => PathBuf::new(),
    }
}

pub(crate) fn is_absolute_runtime_program(program: &str) -> bool {
    Path::new(program).is_absolute()
        || looks_like_windows_rooted_runtime_program(program)
        || is_portable_runtime_program(program)
}

pub(crate) fn is_portable_runtime_program(program: &str) -> bool {
    let program = program.replace('\\', "/").to_ascii_lowercase();
    [
        "~",
        "@config",
        "@local-data",
        "@data",
        "@cache",
        "%userprofile%",
        "%appdata%",
        "%localappdata%",
    ]
    .iter()
    .any(|anchor| program == *anchor || program.starts_with(&format!("{anchor}/")))
}

pub(crate) fn looks_like_windows_rooted_runtime_program(program: &str) -> bool {
    let bytes = program.as_bytes();
    if program.starts_with("\\\\") || program.starts_with('\\') {
        return true;
    }
    bytes.len() >= 3
        && bytes[1] == b':'
        && (bytes[2] == b'\\' || bytes[2] == b'/')
        && bytes[0].is_ascii_alphabetic()
}

pub(crate) fn runtime_program_location_suffix(program: &Path) -> &'static str {
    if program.is_absolute() {
        ""
    } else {
        " on PATH"
    }
}

pub(crate) fn runtime_remediation_hint(
    kind: &ConversationAdapterRuntimeKind,
    program: &Path,
) -> String {
    let runtime_name = runtime_display_name(kind);
    let configured_path_hint = if program.is_absolute() {
        " Verify that the configured path exists and can run --version, or clear the custom runtime path to use PATH."
    } else {
        " Install it and ensure it is available on PATH, or set an absolute runtime path in Settings > Conversations > Conversation Parsers."
    };
    match kind {
        ConversationAdapterRuntimeKind::Node => {
            format!("Install Node.js 20 or newer.{configured_path_hint}")
        }
        ConversationAdapterRuntimeKind::Python => {
            format!("Install Python 3.10 or newer.{configured_path_hint}")
        }
        ConversationAdapterRuntimeKind::Bash => {
            format!("Install bash or configure a bash-compatible shell path.{configured_path_hint}")
        }
        ConversationAdapterRuntimeKind::Executable => {
            format!("Check the executable runtime path for {runtime_name}.")
        }
    }
}

pub(crate) fn runtime_args(kind: &ConversationAdapterRuntimeKind) -> Vec<String> {
    match kind {
        #[cfg(windows)]
        ConversationAdapterRuntimeKind::Python => vec!["-3".to_string()],
        _ => Vec::new(),
    }
}

pub(crate) fn runtime_version_args(kind: &ConversationAdapterRuntimeKind) -> Vec<&'static str> {
    match kind {
        ConversationAdapterRuntimeKind::Node => vec!["--version"],
        #[cfg(windows)]
        ConversationAdapterRuntimeKind::Python => vec!["-3", "--version"],
        #[cfg(not(windows))]
        ConversationAdapterRuntimeKind::Python => vec!["--version"],
        ConversationAdapterRuntimeKind::Bash => vec!["--version"],
        ConversationAdapterRuntimeKind::Executable => Vec::new(),
    }
}

pub(crate) fn runtime_display_name(kind: &ConversationAdapterRuntimeKind) -> &'static str {
    match kind {
        ConversationAdapterRuntimeKind::Node => "node",
        ConversationAdapterRuntimeKind::Python => "python",
        ConversationAdapterRuntimeKind::Bash => "bash",
        ConversationAdapterRuntimeKind::Executable => "executable",
    }
}
