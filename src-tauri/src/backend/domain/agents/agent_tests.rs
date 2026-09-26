use super::*;

#[test]
fn agent_market_protocol_serde() {
    assert_eq!(
        serde_json::to_string(&AgentMarketProtocol::Acp).unwrap(),
        "\"acp\""
    );
    assert_eq!(
        serde_json::to_string(&AgentMarketProtocol::Native).unwrap(),
        "\"native\""
    );
}

#[test]
fn distribution_type_ownership_mapping() {
    assert_eq!(DistributionType::System.ownership(), Ownership::System);
    assert_eq!(DistributionType::Binary.ownership(), Ownership::Managed);
    assert_eq!(DistributionType::Npx.ownership(), Ownership::Managed);
    assert_eq!(DistributionType::Uvx.ownership(), Ownership::Managed);
}
