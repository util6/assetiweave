//! 中立的 conversation read-model projection。
//!
//! 这里只做纯数据转换，不访问 store、文件系统、进程或 application workflow。
pub(crate) mod conversation_cards;
pub(crate) mod conversation_content_nodes;
