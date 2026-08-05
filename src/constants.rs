//! 全局常量集中定义，避免散落在各文件的魔法值。
//! 运行时可配置的值已迁移至 settings.json（见 src/settings.rs）。

pub const SUBSCRIPTION_UA: &str =
    concat!("mihomo-tui/v", env!("CARGO_PKG_VERSION"), " clash-verge");

pub const CONFIG_DIR_NAME: &str = "coclash";
pub const CONFIG_FILE: &str = "config.yaml";
pub const SETTINGS_FILE: &str = "settings.json";
pub const PID_FILE: &str = "mihomo.pid";
pub const MIHOMO_LOG_FILE: &str = "mihomo.log";
