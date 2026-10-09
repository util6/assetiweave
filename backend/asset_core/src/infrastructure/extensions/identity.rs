use schemars::JsonSchema;
use semver::{Version, VersionReq};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PackageKind {
    ConversationAdapter,
    Agent,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub struct PackageIdentity {
    pub kind: PackageKind,
    pub package_id: String,
    #[schemars(with = "String")]
    pub version: Version,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Compatibility {
    pub protocol_version: u32,
    #[schemars(with = "Option<String>")]
    pub core_requirement: Option<VersionReq>,
}

impl Compatibility {
    #[cfg(test)]
    pub fn accepts_core(&self, host: &Version) -> bool {
        self.core_requirement
            .as_ref()
            .is_none_or(|requirement| requirement.matches(host))
    }
}
