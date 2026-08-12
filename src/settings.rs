use crate::constants::DEFAULT_TEST_URL;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::time::Duration;

/// TUI/工具侧设置。
/// mihomo 的监听端口属于 config.yaml（`MihomoConfig`）的职责，由 `state.config` 作为唯一数据源；
/// 这里只保存本程序**连接** mihomo 用的 external-controller 地址（`mihomo_ctrl_addr`），
/// API 地址由它派生（见 `api_url`），避免两处地址脱钩。
/// 缺字段时回落到 `Default`：旧版本 settings.json（缺新增字段）不再整体解析失败，
/// 用户已配置的字段保持原样，只补默认值。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
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

    /// RESTful API 基地址：由 external-controller 地址派生（唯一数据源，避免两处地址脱钩）
    pub fn api_url(&self) -> String {
        format!("http://{}", self.mihomo_ctrl_addr)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_partial_settings_json_uses_defaults() {
        // P0 回归：缺字段的 settings.json 不应整体解析失败；缺失字段取 Settings::default
        let s: Settings = serde_json::from_str(r#"{"mihomo_ctrl_addr":"127.0.0.1:9999"}"#).unwrap();
        assert_eq!(s.mihomo_ctrl_addr, "127.0.0.1:9999");
        assert_eq!(s.test_url, Settings::default().test_url);
        assert_eq!(s.poll_interval_ms, Settings::default().poll_interval_ms);
        // P1-1 回归：API 地址由 ctrl 地址派生
        assert_eq!(s.api_url(), "http://127.0.0.1:9999");
    }
}
