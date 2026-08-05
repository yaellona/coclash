use crate::config::mihomo_config::MihomoConfig;
use crate::settings::Settings;
use std::path::PathBuf;

/// 配置管理器：配置 + 路径 + 设置
#[derive(Debug)]
pub struct ConfigManager {
    pub config: MihomoConfig,
    pub config_path: PathBuf,
    pub settings: Settings,
}
