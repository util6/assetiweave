use schemars::JsonSchema;
use semver::{Version, VersionReq};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub(crate) enum PackageKind {
    ConversationAdapter,
    Agent,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
pub(crate) struct PackageIdentity {
    pub(crate) kind: PackageKind,
    pub(crate) package_id: String,
    #[schemars(with = "String")]
    pub(crate) version: Version,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub(crate) struct Compatibility {
    pub(crate) protocol_version: u32,
    #[schemars(with = "Option<String>")]
    pub(crate) core_requirement: Option<VersionReq>,
}

impl Compatibility {
    pub(crate) fn accepts_core(&self, host: &Version) -> bool {
        self.core_requirement
            .as_ref()
            .is_none_or(|requirement| requirement.matches(host))
    }
}
