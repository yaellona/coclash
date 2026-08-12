use crate::constants::{
    DEFAULT_CTRL_ADDR, DEFAULT_GROUP, DEFAULT_MIXED_PORT, DEFAULT_SOCKS_PORT, SUBSCRIPTION_UA,
};
use crate::error::Error;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

/// 缺字段时回落到 `Default`（即 `default_config()`），旧版本/手写精简配置不会解析失败。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct MihomoConfig {
    pub port: u16,
    #[serde(rename = "socks-port")]
    pub socks_port: u16,
    #[serde(rename = "allow-lan")]
    pub allow_lan: bool,
    pub mode: String,
    #[serde(rename = "log-level")]
    pub log_level: String,
    #[serde(rename = "external-controller")]
    pub external_controller: String,
    #[serde(rename = "geox-url", default)]
    pub geox_url: GeoXUrl,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tun: Option<Tun>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dns: Option<Dns>,
    #[serde(rename = "unified-delay")]
    pub unified_delay: bool,
    #[serde(rename = "keep-alive-interval")]
    pub keep_alive_interval: u32,
    #[serde(rename = "clash-for-android")]
    pub clash_for_android: ClashForAndroid,
    pub sniffer: Sniffer,
    #[serde(rename = "proxy-groups")]
    pub proxy_groups: Vec<ProxyGroup>,
    #[serde(rename = "proxy-providers")]
    pub proxy_providers: Option<IndexMap<String, ProxyProvider>>,
    pub rules: Vec<String>,
}

const GEO_MIRROR: &str = "https://testingcf.jsdelivr.net/gh/MetaCubeX/meta-rules-dat@release";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct GeoXUrl {
    pub geoip: String,
    pub geosite: String,
    pub mmdb: String,
}

impl Default for GeoXUrl {
    fn default() -> Self {
        Self {
            geoip: format!("{GEO_MIRROR}/geoip.dat"),
            geosite: format!("{GEO_MIRROR}/geosite.dat"),
            mmdb: format!("{GEO_MIRROR}/geoip.metadb"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClashForAndroid {
    #[serde(rename = "append-system-dns")]
    pub append_system_dns: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sniffer {
    pub sniff: SniffConfig,
    pub enable: bool,
    #[serde(rename = "force-domain")]
    pub force_domain: Vec<String>,
    #[serde(rename = "skip-domain")]
    pub skip_domain: Vec<String>,
    #[serde(rename = "parse-pure-ip")]
    pub parse_pure_ip: bool,
    #[serde(rename = "force-dns-mapping")]
    pub force_dns_mapping: bool,
    #[serde(rename = "override-destination")]
    pub override_destination: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SniffConfig {
    pub tls: PortConfig,
    pub http: PortConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortConfig {
    pub ports: Vec<String>,
    #[serde(rename = "override-destination")]
    pub override_destination: bool,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Tun {
    pub enable: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stack: Option<String>,
    #[serde(
        default,
        rename = "dns-hijack",
        skip_serializing_if = "Option::is_none"
    )]
    pub dns_hijack: Option<Vec<String>>,
    #[serde(
        default,
        rename = "auto-route",
        skip_serializing_if = "Option::is_none"
    )]
    pub auto_route: Option<bool>,
    #[serde(
        default,
        rename = "auto-redirect",
        skip_serializing_if = "Option::is_none"
    )]
    pub auto_redirect: Option<bool>,
    #[serde(
        default,
        rename = "auto-detect-interface",
        skip_serializing_if = "Option::is_none"
    )]
    pub auto_detect_interface: Option<bool>,
    #[serde(
        default,
        rename = "strict-route",
        skip_serializing_if = "Option::is_none"
    )]
    pub strict_route: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mtu: Option<u32>,
}

impl Tun {
    pub fn default_enabled() -> Self {
        Self {
            enable: true,
            stack: Some("mixed".to_string()),
            dns_hijack: Some(vec!["any:53".to_string(), "tcp://any:53".to_string()]),
            auto_route: Some(true),
            auto_redirect: Some(true),
            auto_detect_interface: Some(true),
            strict_route: Some(true),
            mtu: Some(1500),
        }
    }
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Dns {
    pub enable: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub listen: Option<String>,
    #[serde(
        default,
        rename = "enhanced-mode",
        skip_serializing_if = "Option::is_none"
    )]
    pub enhanced_mode: Option<String>,
    #[serde(
        default,
        rename = "fake-ip-range",
        skip_serializing_if = "Option::is_none"
    )]
    pub fake_ip_range: Option<String>,
    #[serde(
        default,
        rename = "fake-ip-filter",
        skip_serializing_if = "Option::is_none"
    )]
    pub fake_ip_filter: Option<Vec<String>>,
    #[serde(
        default,
        rename = "default-nameserver",
        skip_serializing_if = "Option::is_none"
    )]
    pub default_nameserver: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nameserver: Option<Vec<String>>,
    #[serde(
        default,
        rename = "proxy-server-nameserver",
        skip_serializing_if = "Option::is_none"
    )]
    pub proxy_server_nameserver: Option<Vec<String>>,
    #[serde(
        default,
        rename = "nameserver-policy",
        skip_serializing_if = "Option::is_none"
    )]
    pub nameserver_policy: Option<IndexMap<String, Vec<String>>>,
}

impl Dns {
    pub fn default_enabled() -> Self {
        let mut policy = IndexMap::new();
        policy.insert(
            "geosite:cn,private".to_string(),
            vec!["https://223.5.5.5/dns-query".to_string()],
        );
        policy.insert(
            "geosite:geolocation-!cn".to_string(),
            vec!["https://1.1.1.1/dns-query".to_string()],
        );
        Self {
            enable: true,
            listen: Some("0.0.0.0:1053".to_string()),
            enhanced_mode: Some("fake-ip".to_string()),
            fake_ip_range: Some("198.18.0.1/16".to_string()),
            fake_ip_filter: Some(vec![
                "*.lan".to_string(),
                "*.local".to_string(),
                "+.msftconnecttest.com".to_string(),
                "+.msftncsi.com".to_string(),
                "localhost.ptlogin2.qq.com".to_string(),
                "+.ntp.org".to_string(),
            ]),
            default_nameserver: Some(vec!["223.5.5.5".to_string(), "1.1.1.1".to_string()]),
            nameserver: Some(vec![
                "https://223.5.5.5/dns-query".to_string(),
                "https://1.1.1.1/dns-query".to_string(),
            ]),
            proxy_server_nameserver: Some(vec!["https://223.5.5.5/dns-query".to_string()]),
            nameserver_policy: Some(policy),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyGroup {
    pub name: String,
    #[serde(rename = "type")]
    pub group_type: String,
    pub proxies: Vec<String>,
    #[serde(rename = "use")]
    pub use_list: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyProvider {
    #[serde(rename = "type")]
    pub provider_type: String,
    pub url: String,
    pub interval: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub header: Option<HashMap<String, Vec<String>>>,
}

impl MihomoConfig {
    pub fn default_config() -> Self {
        Self {
            port: DEFAULT_MIXED_PORT,
            socks_port: DEFAULT_SOCKS_PORT,
            allow_lan: true,
            mode: "Rule".to_string(),
            log_level: "info".to_string(),
            external_controller: DEFAULT_CTRL_ADDR.to_string(),
            geox_url: GeoXUrl::default(),
            tun: None,
            dns: None,
            unified_delay: true,
            keep_alive_interval: 360,
            clash_for_android: ClashForAndroid {
                append_system_dns: false,
            },
            sniffer: Sniffer {
                sniff: SniffConfig {
                    tls: PortConfig {
                        ports: vec!["1-65535".to_string()],
                        override_destination: true,
                    },
                    http: PortConfig {
                        ports: vec!["1-65535".to_string()],
                        override_destination: true,
                    },
                },
                enable: true,
                force_domain: vec!["+.netflix.com".to_string()],
                skip_domain: vec!["Mijia Cloud".to_string(), "dlg.io.mi.com".to_string()],
                parse_pure_ip: true,
                force_dns_mapping: true,
                override_destination: true,
            },
            proxy_groups: vec![ProxyGroup {
                name: DEFAULT_GROUP.to_string(),
                group_type: "select".to_string(),
                proxies: vec!["DIRECT".to_string()],
                use_list: vec![],
            }],
            proxy_providers: None,
            rules: vec![
                "GEOSITE,category-ads-all,REJECT".to_string(),
                "GEOSITE,google,Proxy".to_string(),
                "GEOSITE,github,Proxy".to_string(),
                "GEOSITE,telegram,Proxy".to_string(),
                "GEOSITE,twitter,Proxy".to_string(),
                "GEOSITE,facebook,Proxy".to_string(),
                "GEOSITE,youtube,Proxy".to_string(),
                "GEOSITE,netflix,Proxy".to_string(),
                "GEOSITE,openai,Proxy".to_string(),
                "GEOIP,LAN,DIRECT".to_string(),
                "GEOIP,CN,DIRECT".to_string(),
                "MATCH,Proxy".to_string(),
            ],
        }
    }

    /// 策略组名（API 请求组相关端点用），兜底默认组
    pub fn group_name(&self) -> &str {
        self.proxy_groups
            .first()
            .map(|g| g.name.as_str())
            .unwrap_or(DEFAULT_GROUP)
    }

    pub fn provider_key_by_index(&self, index: usize) -> Option<String> {
        self.proxy_providers
            .as_ref()
            .and_then(|p| p.keys().nth(index).cloned())
    }

    pub fn provider_index_by_key(&self, key: &str) -> Option<usize> {
        self.proxy_providers
            .as_ref()
            .and_then(|p| p.get_index_of(key))
    }

    pub fn prepare_switch_provider(&mut self, name: &str) -> Result<(), Error> {
        let exists = self
            .proxy_providers
            .as_ref()
            .map(|providers| providers.contains_key(name))
            .unwrap_or(false);
        if !exists {
            return Err(Error::Config(format!("订阅 '{}' 不存在", name)));
        }
        if let Some(group) = self.proxy_groups.first_mut() {
            group.use_list = vec![name.to_string()];
        }
        Ok(())
    }

    /// 仅改内存；落盘由调用方负责（`save_config`，锁外写盘）
    pub fn set_tun_enabled(&mut self, enabled: bool) {
        if enabled {
            let tun = self.tun.get_or_insert_with(Tun::default_enabled);
            tun.enable = true;
            self.dns.get_or_insert_with(Dns::default_enabled).enable = true;
        } else if let Some(t) = self.tun.as_mut() {
            t.enable = false;
        }
    }

    /// 仅改内存；落盘由调用方负责（`save_config`，锁外写盘）。
    /// 与 `set_tun_enabled` 共用同一份默认 DNS 配置，避免两处实现分叉。
    pub fn set_dns_enabled(&mut self, enabled: bool) {
        if enabled {
            self.dns.get_or_insert_with(Dns::default_enabled).enable = true;
        } else if let Some(d) = self.dns.as_mut() {
            d.enable = false;
        }
    }

    /// 仅改内存（同步从策略组的 use 列表移除，组回落 DIRECT）；落盘由调用方负责
    pub fn remove_provider(&mut self, name: &str) {
        if let Some(providers) = self.proxy_providers.as_mut() {
            providers.shift_remove(name);
        }
        for group in &mut self.proxy_groups {
            group.use_list.retain(|n| n != name);
        }
    }

    /// 仅改内存（重名自动追加序号）；落盘由调用方负责
    pub fn insert_sub(&mut self, url: String, mut sub_name: String) {
        if self.proxy_providers.is_none() {
            self.proxy_providers = Some(IndexMap::new());
        }

        if let Some(ref mut providers) = self.proxy_providers {
            if providers.contains_key(&sub_name) {
                let base = sub_name.clone();
                let mut i = 1;
                loop {
                    let candidate = format!("{base} ({i})");
                    if !providers.contains_key(&candidate) {
                        sub_name = candidate;
                        break;
                    }
                    i += 1;
                }
            }
            let mut header = HashMap::new();
            header.insert("User-Agent".to_string(), vec![SUBSCRIPTION_UA.to_string()]);
            providers.insert(
                sub_name,
                ProxyProvider {
                    provider_type: "http".to_string(),
                    url,
                    interval: 3600,
                    header: Some(header),
                },
            );
        }
    }

    pub fn from_yaml(yaml_str: &str) -> Result<Self, Error> {
        serde_yaml::from_str(yaml_str).map_err(|e| Error::Config(format!("解析YAML失败: {e}")))
    }

    pub fn to_yaml(&self) -> Result<String, Error> {
        serde_yaml::to_string(self).map_err(|e| Error::Config(format!("序列化YAML失败: {e}")))
    }

    pub fn read_from_file(config_path: &PathBuf) -> Result<Self, Error> {
        let content = fs::read_to_string(config_path)
            .map_err(|e| Error::Config(format!("读取文件失败: {e}")))?;
        Self::from_yaml(&content)
    }

    pub fn write_to_path(&self, config_path: &PathBuf) -> Result<(), Error> {
        if let Some(parent) = config_path.parent()
            && !parent.exists()
        {
            fs::create_dir_all(parent).map_err(|e| Error::Config(format!("创建目录失败: {e}")))?;
        }
        let yaml_str = self.to_yaml()?;
        fs::write(config_path, yaml_str)
            .map_err(|e| Error::Config(format!("写入文件失败: {e}")))?;
        Ok(())
    }
}

/// `#[serde(default)]` 的回落：缺字段时取 `default_config()` 的完整默认值
impl Default for MihomoConfig {
    fn default() -> Self {
        Self::default_config()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_yaml_roundtrip() {
        let config = MihomoConfig::default_config();
        let yaml = config.to_yaml().unwrap();
        let config2 = MihomoConfig::from_yaml(&yaml).unwrap();
        assert_eq!(config.port, config2.port);
        assert_eq!(config.socks_port, config2.socks_port);
        assert_eq!(config.proxy_groups.len(), config2.proxy_groups.len());
        assert_eq!(config.rules.len(), config2.rules.len());
        assert_eq!(config.sniffer.enable, config2.sniffer.enable);
        assert_eq!(config.geox_url.geosite, config2.geox_url.geosite);
        assert_eq!(config.geox_url.geoip, config2.geox_url.geoip);
        assert_eq!(config.geox_url.mmdb, config2.geox_url.mmdb);
    }

    #[test]
    fn test_invalid_yaml() {
        let err = MihomoConfig::from_yaml("invalid: yaml: content: [").unwrap_err();
        assert!(err.to_string().contains("解析YAML失败"));
    }

    #[test]
    fn test_geo_url_auto_fill() {
        let config = MihomoConfig::from_yaml(
            "port: 7890\nsocks-port: 7891\nallow-lan: true\nmode: Rule\nlog-level: info\nexternal-controller: :9090\nunified-delay: true\nkeep-alive-interval: 360\nclash-for-android:\n  append-system-dns: false\nsniffer:\n  enable: true\n  force-domain: []\n  skip-domain: []\n  parse-pure-ip: true\n  force-dns-mapping: true\n  override-destination: true\n  sniff:\n    tls:\n      ports: []\n      override-destination: true\n    http:\n      ports: []\n      override-destination: true\nproxy-groups: []\nrules: []\n",
        )
        .unwrap();
        assert!(config.geox_url.geosite.contains("jsdelivr.net"));
        assert!(config.geox_url.geoip.contains("jsdelivr.net"));
        assert!(config.geox_url.mmdb.contains("jsdelivr.net"));
        let yaml = config.to_yaml().unwrap();
        assert!(yaml.contains("geox-url:"));
        assert!(yaml.contains("geosite:"));
    }

    #[test]
    fn test_partial_config_yaml_uses_defaults() {
        // P0 回归：缺字段的 config.yaml 不应解析失败；缺失字段取 default_config 的值
        let config = MihomoConfig::from_yaml("port: 1234\n").unwrap();
        assert_eq!(config.port, 1234);
        assert_eq!(config.socks_port, MihomoConfig::default_config().socks_port);
        assert_eq!(config.mode, "Rule");
        assert!(!config.rules.is_empty());
        assert!(config.tun.is_none());
    }

    #[test]
    fn test_insert_sub_dedup() {
        let mut config = MihomoConfig::default_config();
        config.insert_sub("https://example.com/sub".to_string(), "订阅".to_string());
        config.insert_sub("https://example.com/sub".to_string(), "订阅".to_string());
        let names: Vec<String> = config
            .proxy_providers
            .as_ref()
            .unwrap()
            .keys()
            .cloned()
            .collect();
        assert_eq!(names, vec!["订阅".to_string(), "订阅 (1)".to_string()]);
    }

    #[test]
    fn test_group_name_fallback() {
        let config = MihomoConfig::default_config();
        assert_eq!(config.group_name(), "Proxy");
        let mut config2 = MihomoConfig::default_config();
        config2.proxy_groups.clear();
        assert_eq!(config2.group_name(), "Proxy");
    }
}
