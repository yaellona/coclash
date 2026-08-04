use crate::command::mihomo;
use crate::config::mihomo_config::MihomoConfig;
use crate::settings::Settings;
use std::io::Write;

#[test]
fn test_pidfile_roundtrip() {
    let dir = tempfile::TempDir::new().unwrap();
    mihomo::save_pid(dir.path(), 12345).unwrap();
    assert_eq!(mihomo::load_pidfile(dir.path()), Some(12345));
    mihomo::clear_pidfile(dir.path());
    assert_eq!(mihomo::load_pidfile(dir.path()), None);
}

#[test]
fn test_pidfile_invalid_content() {
    let dir = tempfile::TempDir::new().unwrap();
    std::fs::write(dir.path().join(crate::constants::PID_FILE), "not-a-pid").unwrap();
    assert_eq!(mihomo::load_pidfile(dir.path()), None);
}

#[test]
fn test_is_pid_alive() {
    assert!(mihomo::is_pid_alive(std::process::id()));
    assert!(!mihomo::is_pid_alive(u32::MAX));
}

#[test]
fn test_config_yaml_roundtrip() {
    let config = MihomoConfig::default_config(&Settings::default());
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
    assert!(err.contains("解析YAML失败"));
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
fn test_insert_sub_dedup() {
    let mut config = MihomoConfig::default_config(&Settings::default());
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("config.yaml");
    config
        .insert_sub("https://example.com/sub".to_string(), "订阅".to_string(), &path)
        .unwrap();
    config
        .insert_sub("https://example.com/sub".to_string(), "订阅".to_string(), &path)
        .unwrap();
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
fn test_mihomo_log_tail() {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("mihomo.log");
    let mut f = std::fs::File::create(&path).unwrap();
    for i in 0..100 {
        writeln!(f, "line {i}").unwrap();
    }
    drop(f);
    let lines = crate::app::mihomo_log::read_tail(&path, 128 * 1024, 10);
    assert_eq!(lines.len(), 10);
    assert_eq!(lines[0], "line 90");
    assert_eq!(lines[9], "line 99");
}
