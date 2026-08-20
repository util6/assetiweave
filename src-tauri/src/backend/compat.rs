//! Transitional result types kept below the Application boundary.

/// Plain string errors used by legacy infrastructure and adapter helpers.
///
/// New application workflows must return `runtime::AppResult` instead.
pub(crate) type LegacyResult<T> = Result<T, String>;
