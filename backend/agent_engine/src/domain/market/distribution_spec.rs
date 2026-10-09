use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::types::{DistributionType, Ownership};

#[derive(Clone, Debug, Deserialize, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Target {
    pub os: String,
    pub arch: String,
}

#[derive(Clone, Debug, Eq, JsonSchema, PartialEq, Serialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum Distribution {
    #[serde(rename = "system")]
    System {
        id: String,
        priority: u32,
        command_candidates: Vec<String>,
        version_args: Vec<String>,
        #[serde(default)]
        version_range: String,
        launch_args: Vec<String>,
        #[serde(default)]
        model_discovery_args: Option<Vec<String>>,
        #[serde(default)]
        session_cleanup_args: Option<Vec<String>>,
        #[serde(default)]
        session_cleanup_not_found_markers: Vec<String>,
    },
    #[serde(rename = "binary")]
    Binary {
        id: String,
        priority: u32,
        target: Target,
        archive: String,
        url: String,
        sha256: String,
        #[serde(default)]
        size: Option<u64>,
        executable: String,
        launch_args: Vec<String>,
        #[serde(default)]
        model_discovery_args: Option<Vec<String>>,
        #[serde(default)]
        session_cleanup_args: Option<Vec<String>>,
        #[serde(default)]
        session_cleanup_not_found_markers: Vec<String>,
    },
    #[serde(rename = "npx")]
    Npx {
        id: String,
        priority: u32,
        package: String,
        version: String,
        bin: String,
        launch_args: Vec<String>,
        #[serde(default)]
        node_range: Option<String>,
        #[serde(default)]
        model_discovery_args: Option<Vec<String>>,
        #[serde(default)]
        session_cleanup_args: Option<Vec<String>>,
        #[serde(default)]
        session_cleanup_not_found_markers: Vec<String>,
    },
    #[serde(rename = "uvx")]
    Uvx {
        id: String,
        priority: u32,
        package: String,
        version: String,
        command: String,
        launch_args: Vec<String>,
        #[serde(default)]
        python_range: Option<String>,
        #[serde(default)]
        model_discovery_args: Option<Vec<String>>,
        #[serde(default)]
        session_cleanup_args: Option<Vec<String>>,
        #[serde(default)]
        session_cleanup_not_found_markers: Vec<String>,
    },
}

impl<'de> Deserialize<'de> for Distribution {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = serde_json::Value::deserialize(deserializer)?;
        let kind = value
            .get("type")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| serde::de::Error::custom("distribution type is required"))?;
        match kind {
            "system" => {
                let fields: SystemDistributionFields =
                    serde_json::from_value(value).map_err(serde::de::Error::custom)?;
                Ok(Self::System {
                    id: fields.id,
                    priority: fields.priority,
                    command_candidates: fields.command_candidates,
                    version_args: fields.version_args,
                    version_range: fields.version_range,
                    launch_args: fields.launch_args,
                    model_discovery_args: fields.model_discovery_args,
                    session_cleanup_args: fields.session_cleanup_args,
                    session_cleanup_not_found_markers: fields.session_cleanup_not_found_markers,
                })
            }
            "binary" => {
                let fields: BinaryDistributionFields =
                    serde_json::from_value(value).map_err(serde::de::Error::custom)?;
                Ok(Self::Binary {
                    id: fields.id,
                    priority: fields.priority,
                    target: fields.target,
                    archive: fields.archive,
                    url: fields.url,
                    sha256: fields.sha256,
                    size: fields.size,
                    executable: fields.executable,
                    launch_args: fields.launch_args,
                    model_discovery_args: fields.model_discovery_args,
                    session_cleanup_args: fields.session_cleanup_args,
                    session_cleanup_not_found_markers: fields.session_cleanup_not_found_markers,
                })
            }
            "npx" => {
                let fields: NpxDistributionFields =
                    serde_json::from_value(value).map_err(serde::de::Error::custom)?;
                Ok(Self::Npx {
                    id: fields.id,
                    priority: fields.priority,
                    package: fields.package,
                    version: fields.version,
                    bin: fields.bin,
                    launch_args: fields.launch_args,
                    node_range: fields.node_range,
                    model_discovery_args: fields.model_discovery_args,
                    session_cleanup_args: fields.session_cleanup_args,
                    session_cleanup_not_found_markers: fields.session_cleanup_not_found_markers,
                })
            }
            "uvx" => {
                let fields: UvxDistributionFields =
                    serde_json::from_value(value).map_err(serde::de::Error::custom)?;
                Ok(Self::Uvx {
                    id: fields.id,
                    priority: fields.priority,
                    package: fields.package,
                    version: fields.version,
                    command: fields.command,
                    launch_args: fields.launch_args,
                    python_range: fields.python_range,
                    model_discovery_args: fields.model_discovery_args,
                    session_cleanup_args: fields.session_cleanup_args,
                    session_cleanup_not_found_markers: fields.session_cleanup_not_found_markers,
                })
            }
            other => Err(serde::de::Error::custom(format!(
                "unsupported distribution type: {other}"
            ))),
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SystemDistributionFields {
    id: String,
    priority: u32,
    command_candidates: Vec<String>,
    version_args: Vec<String>,
    version_range: String,
    launch_args: Vec<String>,
    #[serde(default)]
    model_discovery_args: Option<Vec<String>>,
    #[serde(default)]
    session_cleanup_args: Option<Vec<String>>,
    #[serde(default)]
    session_cleanup_not_found_markers: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BinaryDistributionFields {
    id: String,
    priority: u32,
    target: Target,
    archive: String,
    url: String,
    sha256: String,
    #[serde(default)]
    size: Option<u64>,
    executable: String,
    launch_args: Vec<String>,
    #[serde(default)]
    model_discovery_args: Option<Vec<String>>,
    #[serde(default)]
    session_cleanup_args: Option<Vec<String>>,
    #[serde(default)]
    session_cleanup_not_found_markers: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct NpxDistributionFields {
    id: String,
    priority: u32,
    package: String,
    version: String,
    bin: String,
    launch_args: Vec<String>,
    #[serde(default)]
    node_range: Option<String>,
    #[serde(default)]
    model_discovery_args: Option<Vec<String>>,
    #[serde(default)]
    session_cleanup_args: Option<Vec<String>>,
    #[serde(default)]
    session_cleanup_not_found_markers: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UvxDistributionFields {
    id: String,
    priority: u32,
    package: String,
    version: String,
    command: String,
    launch_args: Vec<String>,
    #[serde(default)]
    python_range: Option<String>,
    #[serde(default)]
    model_discovery_args: Option<Vec<String>>,
    #[serde(default)]
    session_cleanup_args: Option<Vec<String>>,
    #[serde(default)]
    session_cleanup_not_found_markers: Vec<String>,
}

impl Distribution {
    pub fn id(&self) -> &str {
        match self {
            Self::System { id, .. }
            | Self::Binary { id, .. }
            | Self::Npx { id, .. }
            | Self::Uvx { id, .. } => id,
        }
    }

    pub fn distribution_type(&self) -> DistributionType {
        match self {
            Self::System { .. } => DistributionType::System,
            Self::Binary { .. } => DistributionType::Binary,
            Self::Npx { .. } => DistributionType::Npx,
            Self::Uvx { .. } => DistributionType::Uvx,
        }
    }

    pub fn launch_args(&self) -> &[String] {
        match self {
            Self::System { launch_args, .. }
            | Self::Binary { launch_args, .. }
            | Self::Npx { launch_args, .. }
            | Self::Uvx { launch_args, .. } => launch_args,
        }
    }

    pub fn session_cleanup_args(&self) -> Option<&[String]> {
        match self {
            Self::System {
                session_cleanup_args,
                ..
            }
            | Self::Binary {
                session_cleanup_args,
                ..
            }
            | Self::Npx {
                session_cleanup_args,
                ..
            }
            | Self::Uvx {
                session_cleanup_args,
                ..
            } => session_cleanup_args.as_deref(),
        }
    }

    pub fn session_cleanup_not_found_markers(&self) -> &[String] {
        match self {
            Self::System {
                session_cleanup_not_found_markers,
                ..
            }
            | Self::Binary {
                session_cleanup_not_found_markers,
                ..
            }
            | Self::Npx {
                session_cleanup_not_found_markers,
                ..
            }
            | Self::Uvx {
                session_cleanup_not_found_markers,
                ..
            } => session_cleanup_not_found_markers,
        }
    }

    #[cfg(test)]
    pub fn ownership(&self) -> Ownership {
        self.distribution_type().ownership()
    }
}
