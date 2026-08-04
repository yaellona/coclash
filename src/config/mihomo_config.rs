use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_yaml;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use crate::constants::SUBSCRIPTION_UA;
use crate::settings::Settings;

#[derive(Debug, Serialize, Deserialize)]
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

#[derive(Debug, Serialize, Deserialize)]
pub struct ClashForAndroid {
    #[serde(rename = "append-system-dns")]
    pub append_system_dns: bool,
}

#[derive(Debug, Serialize, Deserialize)]
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

#[derive(Debug, Serialize, Deserialize)]
pub struct SniffConfig {
    pub tls: PortConfig,
    pub http: PortConfig,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct PortConfig {
    pub ports: Vec<String>,
    #[serde(rename = "override-destination")]
    pub override_destination: bool,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Tun {
    pub enable: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stack: Option<String>,
    #[serde(default, rename = "dns-hijack", skip_serializing_if = "Option::is_none")]
    pub dns_hijack: Option<Vec<String>>,
    #[serde(default, rename = "auto-route", skip_serializing_if = "Option::is_none")]
    pub auto_route: Option<bool>,
    #[serde(default, rename = "auto-redirect", skip_serializing_if = "Option::is_none")]
    pub auto_redirect: Option<bool>,
    #[serde(default, rename = "auto-detect-interface", skip_serializing_if = "Option::is_none")]
    pub auto_detect_interface: Option<bool>,
    #[serde(default, rename = "strict-route", skip_serializing_if = "Option::is_none")]
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

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Dns {
    pub enable: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub listen: Option<String>,
    #[serde(default, rename = "enhanced-mode", skip_serializing_if = "Option::is_none")]
    pub enhanced_mode: Option<String>,
    #[serde(default, rename = "fake-ip-range", skip_serializing_if = "Option::is_none")]
    pub fake_ip_range: Option<String>,
    #[serde(default, rename = "fake-ip-filter", skip_serializing_if = "Option::is_none")]
    pub fake_ip_filter: Option<Vec<String>>,
    #[serde(default, rename = "default-nameserver", skip_serializing_if = "Option::is_none")]
    pub default_nameserver: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nameserver: Option<Vec<String>>,
    #[serde(default, rename = "proxy-server-nameserver", skip_serializing_if = "Option::is_none")]
    pub proxy_server_nameserver: Option<Vec<String>>,
    #[serde(default, rename = "nameserver-policy", skip_serializing_if = "Option::is_none")]
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

#[derive(Debug, Serialize, Deserialize)]
pub struct ProxyGroup {
    pub name: String,
    #[serde(rename = "type")]
    pub group_type: String,
    pub proxies: Vec<String>,
    #[serde(rename = "use")]
    pub use_list: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ProxyProvider {
    #[serde(rename = "type")]
    pub provider_type: String,
    pub url: String,
    pub interval: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub header: Option<HashMap<String, Vec<String>>>,
}

impl MihomoConfig {
    pub fn default_config(settings: &Settings) -> Self {
        Self {
            port: settings.mixed_port,
            socks_port: settings.socks_port,
            allow_lan: true,
            mode: "Rule".to_string(),
            log_level: "info".to_string(),
            external_controller: settings.external_controller.clone(),
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
                name: "Proxy".to_string(),
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

    pub fn prepare_switch_provider(
        &mut self,
        name: &str,
        config_path: &PathBuf,
    ) -> Result<(), String> {
        let exists = self
            .proxy_providers
            .as_ref()
            .map(|providers| providers.contains_key(name))
            .unwrap_or(false);
        if !exists {
            return Err(format!("代理商 '{}' 不存在", name));
        }
        if let Some(group) = self.proxy_groups.first_mut() {
            group.use_list = vec![name.to_string()];
        }
        self.write_to_path(config_path)?;
        Ok(())
    }

    pub fn set_tun_enabled(
        &mut self,
        enabled: bool,
        config_path: &PathBuf,
    ) -> Result<(), String> {
        if enabled {
            let tun = self.tun.get_or_insert_with(Tun::default_enabled);
            tun.enable = true;
            self.dns.get_or_insert_with(Dns::default_enabled).enable = true;
        } else if let Some(t) = self.tun.as_mut() {
            t.enable = false;
        }
        self.write_to_path(config_path)?;
        Ok(())
    }

    pub fn insert_sub(
        &mut self,
        url: String,
        mut sub_name: String,
        config_path: &PathBuf,
    ) -> Result<(), String> {
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
        self.write_to_path(config_path)?;
        Ok(())
    }

    pub fn from_yaml(yaml_str: &str) -> Result<Self, String> {
        serde_yaml::from_str(yaml_str).map_err(|e| format!("解析YAML失败: {}", e))
    }

    pub fn to_yaml(&self) -> Result<String, String> {
        serde_yaml::to_string(self).map_err(|e| format!("序列化YAML失败: {}", e))
    }

    pub fn read_from_file(config_path: &PathBuf) -> Result<Self, String> {
        let content =
            fs::read_to_string(config_path).map_err(|e| format!("读取文件失败: {}", e))?;

        Self::from_yaml(&content)
    }

    pub fn write_to_path(&self, config_path: &PathBuf) -> Result<(), String> {
        if let Some(parent) = config_path.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent).map_err(|e| format!("创建目录失败: {}", e))?;
            }
        }
        let yaml_str = self.to_yaml()?;
        fs::write(config_path, yaml_str).map_err(|e| format!("写入文件失败: {}", e))?;
        Ok(())
    }
}
