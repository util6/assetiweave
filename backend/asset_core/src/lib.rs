pub mod application;
pub mod domain;
pub mod infrastructure;
pub mod store;

// 内部自引用别名，确保 asset_core 内部既有的 crate::backend::... 路径零改动无缝编译通过
pub use self as backend;
