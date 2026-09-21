//! mihomo RESTful API 客户端：单一共享 client，逐调用设置超时。
use crate::constants::DEFAULT_GROUP;
use crate::error::Error;
use crate::settings::Settings;
use serde::Deserialize;
use serde::de::DeserializeOwned;
use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

/// 用于接收 mihomo 策略组回复的节点报告（serde 反序列化目标，字段需全量保留）
#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct ProxyReport {
    pub alive: bool,
    pub all: Vec<String>,
    #[serde(rename = "dialer-proxy")]
    pub dialer_proxy: String,
    pub hidden: bool,
    pub icon: String,
    pub interface: String,
    pub name: String,
    pub now: String,
    #[serde(rename = "type")]
    pub node_type: String,
}

/// `/version` 响应（心跳展示内核版本）
#[derive(Debug, Deserialize)]
pub struct VersionReport {
    #[serde(default)]
    pub version: String,
}

/// `/configs` 响应（心跳只取运行时展示需要的字段）
#[derive(Debug, Deserialize)]
pub struct ConfigsReport {
    #[serde(rename = "mixed-port", default)]
    pub mixed_port: u16,
    #[serde(rename = "socks-port", default)]
    pub socks_port: u16,
    #[serde(default)]
    pub mode: String,
    #[serde(default)]
    pub tun: Option<EnableReport>,
    #[serde(default)]
    pub dns: Option<EnableReport>,
}

/// 带 `enable` 字段的子配置（tun/dns 共用）
#[derive(Debug, Deserialize)]
pub struct EnableReport {
    #[serde(default)]
    pub enable: bool,
}

/// `/connections` 响应（心跳只取累计流量，连接明细忽略）
#[derive(Debug, Deserialize)]
pub struct ConnectionsReport {
    #[serde(rename = "uploadTotal", default)]
    pub upload_total: u64,
    #[serde(rename = "downloadTotal", default)]
    pub download_total: u64,
}

pub struct ApiClient {
    client: reqwest::Client,
    base_url: String,
    group: String,
    test_url: String,
    delay_timeout_ms: u64,
    delay_http_timeout: Duration,
    http_timeout: Duration,
}

impl ApiClient {
    pub fn new(settings: &Settings, group: &str) -> Result<Self, Error> {
        let client = reqwest::Client::builder()
            .no_proxy()
            .build()
            .map_err(|e| Error::Api(format!("创建HTTP客户端失败: {e}")))?;
        Ok(Self {
            client,
            base_url: settings.api_url(),
            group: if group.is_empty() {
                DEFAULT_GROUP.to_string()
            } else {
                group.to_string()
            },
            test_url: settings.test_url.clone(),
            delay_timeout_ms: settings.delay_timeout_ms,
            delay_http_timeout: settings.delay_http_timeout(),
            http_timeout: settings.http_timeout(),
        })
    }

    /// GET + JSON 反序列化（统一超时与错误文案）
    async fn get_json<T: DeserializeOwned>(&self, path: &str) -> Result<T, Error> {
        let url = format!("{}{path}", self.base_url);
        let body = self
            .client
            .get(url)
            .timeout(self.http_timeout)
            .send()
            .await
            .map_err(|e| Error::Api(format!("请求失败: {e}")))?
            .text()
            .await
            .map_err(|e| Error::Api(format!("读取响应失败: {e}")))?;
        serde_json::from_str(&body).map_err(|e| Error::Api(format!("解析响应失败: {e}")))
    }

    pub async fn get_proxy(&self) -> Result<ProxyReport, Error> {
        self.get_json(&format!("/proxies/{}", self.group)).await
    }

    pub async fn get_version(&self) -> Result<VersionReport, Error> {
        self.get_json("/version").await
    }

    pub async fn get_configs(&self) -> Result<ConfigsReport, Error> {
        self.get_json("/configs").await
    }

    pub async fn get_connections(&self) -> Result<ConnectionsReport, Error> {
        self.get_json("/connections").await
    }

    pub async fn fetch_delays(&self) -> Result<HashMap<String, u32>, Error> {
        let path = format!(
            "/group/{}/delay?timeout={}&url={}",
            self.group, self.delay_timeout_ms, self.test_url
        );
        let url = format!("{}{path}", self.base_url);
        let body = self
            .client
            .get(url)
            .timeout(self.delay_http_timeout)
            .send()
            .await
            .map_err(|e| Error::Api(format!("测速请求失败: {e}")))?
            .text()
            .await
            .map_err(|e| Error::Api(format!("读取响应失败: {e}")))?;
        serde_json::from_str(&body).map_err(|e| Error::Api(format!("解析延迟失败: {e}")))
    }

    pub async fn switch_node(&self, name: &str) -> Result<(), Error> {
        let url = format!("{}/proxies/{}", self.base_url, self.group);
        let body = serde_json::json!({ "name": name });
        let resp = self
            .client
            .put(url)
            .timeout(self.http_timeout)
            .json(&body)
            .send()
            .await
            .map_err(|e| Error::Api(format!("切换节点失败: {e}")))?;
        if !resp.status().is_success() {
            return Err(Error::Api(format!(
                "切换节点失败：API返回状态码 {}",
                resp.status()
            )));
        }
        Ok(())
    }

    pub async fn reload_config(&self, path: &Path) -> Result<(), Error> {
        let url = format!("{}/configs?force=true", self.base_url);
        let body = serde_json::json!({ "path": path.to_string_lossy(), "payload": "" });
        let resp = self
            .client
            .put(url)
            .timeout(self.http_timeout)
            .json(&body)
            .send()
            .await
            .map_err(|e| Error::Api(format!("重载配置失败: {e}")))?;
        if !resp.status().is_success() {
            return Err(Error::Api(format!(
                "重载配置失败：API返回状态码 {}",
                resp.status()
            )));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_configs_report_partial() {
        // /configs 字段缺失时用默认值，不整段解析失败
        let cfg: ConfigsReport = serde_json::from_str(r#"{"mode":"rule"}"#).unwrap();
        assert_eq!(cfg.mode, "rule");
        assert_eq!(cfg.mixed_port, 0);
        assert!(cfg.tun.is_none());
    }

    #[test]
    fn test_configs_report_full() {
        let cfg: ConfigsReport = serde_json::from_str(
            r#"{"mixed-port":7890,"socks-port":7891,"mode":"global","tun":{"enable":true},"dns":{"enable":false}}"#,
        )
        .unwrap();
        assert_eq!(cfg.mixed_port, 7890);
        assert_eq!(cfg.socks_port, 7891);
        assert!(cfg.tun.unwrap().enable);
        assert!(!cfg.dns.unwrap().enable);
    }

    #[test]
    fn test_connections_report() {
        let c: ConnectionsReport =
            serde_json::from_str(r#"{"uploadTotal":12,"downloadTotal":34,"connections":[]}"#)
                .unwrap();
        assert_eq!(c.upload_total, 12);
        assert_eq!(c.download_total, 34);
    }

    #[test]
    fn test_version_report() {
        let v: VersionReport =
            serde_json::from_str(r#"{"version":"v1.18.0","meta":true}"#).unwrap();
        assert_eq!(v.version, "v1.18.0");
    }
}
