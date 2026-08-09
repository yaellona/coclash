use crate::constants::DEFAULT_TEST_URL;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::time::Duration;

/// TUI/工具侧设置。
/// 端口类参数不属于这里——它们是 config.yaml（`MihomoConfig`）的职责，
/// 由 `state.config` 作为唯一数据源。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub mihomo_api: String,
    pub mihomo_ctrl_addr: String,
    pub test_url: String,
    pub delay_timeout_ms: u64,
    pub http_timeout_ms: u64,
    pub delay_http_timeout_ms: u64,
    pub provider_retry: u32,
    pub provider_retry_interval_ms: u64,
    pub poll_interval_ms: u64,
    pub mihomo_exe: String,
    pub channel_capacity: usize,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            mihomo_api: "http://127.0.0.1:9090".to_string(),
            mihomo_ctrl_addr: "127.0.0.1:9090".to_string(),
            test_url: DEFAULT_TEST_URL.to_string(),
            delay_timeout_ms: 5000,
            http_timeout_ms: 5000,
            delay_http_timeout_ms: 6000,
            provider_retry: 6,
            provider_retry_interval_ms: 500,
            poll_interval_ms: 100,
            mihomo_exe: String::new(),
            channel_capacity: 16,
        }
    }
}

impl Settings {
    /// 读取或创建；文件缺失时写入默认值，**解析失败时同样用默认值并重建文件**（自愈）。
    pub fn load_or_create(path: &Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(content) => match serde_json::from_str::<Settings>(&content) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("settings.json 解析失败({e})，使用默认值并重建文件");
                    let default = Settings::default();
                    write_default(path, &default);
                    default
                }
            },
            Err(_) => {
                let default = Settings::default();
                write_default(path, &default);
                default
            }
        }
    }

    pub fn http_timeout(&self) -> Duration {
        Duration::from_millis(self.http_timeout_ms)
    }

    pub fn delay_http_timeout(&self) -> Duration {
        Duration::from_millis(self.delay_http_timeout_ms)
    }

    pub fn provider_retry_interval(&self) -> Duration {
        Duration::from_millis(self.provider_retry_interval_ms)
    }

    pub fn poll_interval(&self) -> Duration {
        Duration::from_millis(self.poll_interval_ms)
    }
}

fn write_default(path: &Path, settings: &Settings) {
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    match serde_json::to_string_pretty(settings) {
        Ok(json) => {
            let _ = std::fs::write(path, json);
        }
        Err(e) => eprintln!("序列化默认 settings.json 失败: {e}"),
    }
}
