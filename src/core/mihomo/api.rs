//! mihomo RESTful API 客户端：单一共享 client，逐调用设置超时。
use crate::constants::{DEFAULT_GROUP, SUBSCRIPTION_UA};
use crate::error::Error;
use crate::settings::Settings;
use serde::Deserialize;
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

    pub async fn get_proxy(&self) -> Result<ProxyReport, Error> {
        let url = format!("{}/proxies/{}", self.base_url, self.group);
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
        serde_json::from_str(&body).map_err(|e| Error::Api(format!("解析节点失败: {e}")))
    }

    pub async fn fetch_delays(&self) -> Result<HashMap<String, u32>, Error> {
        let url = format!(
            "{}/group/{}/delay?timeout={}&url={}",
            self.base_url, self.group, self.delay_timeout_ms, self.test_url
        );
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

    /// 用 flclash 的方式获取订阅名称（content-disposition 或域名兜底）
    pub async fn get_provider_name(&self, url: &str) -> Result<String, Error> {
        let domain = url::Url::parse(url)
            .ok()
            .and_then(|u| u.host_str().map(|s| s.to_string()));
        let resp = self
            .client
            .get(url)
            .timeout(self.delay_http_timeout)
            .header("User-Agent", SUBSCRIPTION_UA)
            .send()
            .await;
        let cd = match &resp {
            Ok(resp) => resp
                .headers()
                .get("content-disposition")
                .and_then(|v| v.to_str().ok())
                .unwrap_or(""),
            Err(e) => {
                if let Some(d) = domain {
                    return Ok(d);
                }
                return Err(Error::Api(format!("请求失败: {e}")));
            }
        };
        if let Some(name) = parse_content_disposition(cd) {
            return Ok(name);
        }
        if let Some(d) = domain {
            return Ok(d);
        }
        Err(Error::Api("无法解析订阅名称".to_string()))
    }
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut result = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let Ok(byte) = u8::from_str_radix(
                std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("xx"),
                16,
            )
        {
            result.push(byte);
            i += 3;
            continue;
        }
        result.push(bytes[i]);
        i += 1;
    }
    String::from_utf8(result).unwrap_or_default()
}

fn parse_content_disposition(cd: &str) -> Option<String> {
    for part in cd.split(';') {
        let p = part.trim();
        if p.to_lowercase().starts_with("filename*=") {
            let val = &p[10..];
            let segs: Vec<&str> = val.split('\'').collect();
            let encoded = if segs.len() >= 3 { segs[2] } else { val };
            return Some(percent_decode(encoded));
        }
    }
    for part in cd.split(';') {
        let p = part.trim();
        if p.to_lowercase().starts_with("filename=") {
            let val = &p[9..];
            return Some(val.trim_matches('"').trim_matches('\'').to_string());
        }
    }
    None
}
