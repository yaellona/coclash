pub mod api;
pub mod binary;
pub mod process;

pub use api::ApiClient;
pub use process::{MihomoStatus, detect_status, start_mihomo, stop_mihomo, tun_capability_warning};

// 测试所需（非 test 构建下会被视为未使用）
#[cfg(test)]
pub(crate) use binary::BinarySource;
