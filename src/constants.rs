//! 全局常量集中定义，避免散落在各文件的魔法值。
//! 运行时可配置的值已迁移至 settings.json（见 src/settings.rs）。

pub const SUBSCRIPTION_UA: &str =
    concat!("mihomo-tui/v", env!("CARGO_PKG_VERSION"), " clash-verge");

pub const CONFIG_DIR_NAME: &str = "coclash";
pub const CONFIG_FILE: &str = "config.yaml";
pub const SETTINGS_FILE: &str = "settings.json";
pub const MIHOMO_LOG_FILE: &str = "mihomo.log";

/// 默认监听端口（config.yaml 缺失时生成默认配置用）
pub const DEFAULT_MIXED_PORT: u16 = 7890;
pub const DEFAULT_SOCKS_PORT: u16 = 7891;
/// mihomo external-controller 默认监听地址
pub const DEFAULT_CTRL_ADDR: &str = ":9090";
/// 默认策略组名（API 请求组相关端点用）
pub const DEFAULT_GROUP: &str = "Proxy";
/// 默认测速 URL
pub const DEFAULT_TEST_URL: &str = "https://www.gstatic.com/generate_204";
