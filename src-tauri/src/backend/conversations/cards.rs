//! Compatibility re-export for conversation-domain callers.
//!
//! The implementation lives in the neutral projection module so persistence
//! code does not depend on the conversations domain implementation.
pub(crate) use crate::backend::projection::conversation_cards::*;
