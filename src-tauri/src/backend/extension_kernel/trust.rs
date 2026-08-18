pub(crate) trait TrustGate: Send + Sync {
    fn can_enable(&self) -> bool;
    fn needs_confirmation(&self) -> bool;
    fn integrity_changed(&self) -> bool;
}
