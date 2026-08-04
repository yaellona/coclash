use serde::{Deserialize, Serialize};
use std::path::Path;
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    pub mihomo_api: String,
    pub mihomo_ctrl_addr: String,
    pub mixed_port: u16,
    pub socks_port: u16,
    pub test_url: String,
    pub delay_timeout_ms: u64,
    pub external_controller: String,
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
            mixed_port: 7890,
            socks_port: 7891,
            test_url: "https://www.gstatic.com/generate_204".to_string(),
            delay_timeout_ms: 5000,
            external_controller: ":9090".to_string(),
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
    pub fn load_or_create(path: &Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(content) => match serde_json::from_str::<Settings>(&content) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("settings.json 解析失败,使用默认值: {e}");
                    Settings::default()
                }
            },
            Err(_) => {
                let default = Settings::default();
                if let Some(parent) = path.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                match serde_json::to_string_pretty(&default) {
                    Ok(json) => {
                        if std::fs::write(path, json).is_err() {
                            eprintln!("无法写入默认 settings.json");
                        }
                    }
                    Err(e) => eprintln!("序列化默认 settings.json 失败: {e}"),
                }
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
