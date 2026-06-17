pub(crate) mod policy;
pub(crate) mod protocol;
pub(crate) mod registry;
pub(crate) mod runtime;
mod transport;

pub(crate) use transport::run_stdio;
