pub(crate) trait TrustGate: Send + Sync {
    #[cfg(test)]
    fn can_enable(&self) -> bool;
    fn needs_confirmation(&self) -> bool;
    #[cfg(test)]
    fn integrity_changed(&self) -> bool;
}
