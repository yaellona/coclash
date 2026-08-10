use crate::core::mihomo;
use crate::core::config::mihomo_config::MihomoConfig;
use crate::settings::Settings;
use std::io::Write;

#[test]
fn test_pidfile_roundtrip() {
    let dir = tempfile::TempDir::new().unwrap();
    mihomo::process::save_pid(dir.path(), 12345).unwrap();
    assert_eq!(mihomo::process::load_pidfile(dir.path()), Some(12345));
    mihomo::process::clear_pidfile(dir.path());
    assert_eq!(mihomo::process::load_pidfile(dir.path()), None);
}

#[test]
fn test_pidfile_invalid_content() {
    let dir = tempfile::TempDir::new().unwrap();
    std::fs::write(dir.path().join(crate::constants::PID_FILE), "not-a-pid").unwrap();
    assert_eq!(mihomo::process::load_pidfile(dir.path()), None);
}

#[test]
fn test_pidfile_elevated_roundtrip() {
    let dir = tempfile::TempDir::new().unwrap();
    mihomo::process::save_pid(dir.path(), 12345).unwrap();
    assert_eq!(mihomo::process::load_pidfile(dir.path()), Some(12345));
    assert_eq!(
        mihomo::process::load_pidfile_elevated(dir.path()),
        Some((12345, false))
    );
    std::fs::write(dir.path().join(crate::constants::PID_FILE), "12345:1").unwrap();
    assert_eq!(
        mihomo::process::load_pidfile_elevated(dir.path()),
        Some((12345, true))
    );
    mihomo::process::clear_pidfile(dir.path());
    assert_eq!(mihomo::process::load_pidfile_elevated(dir.path()), None);
}

#[test]
fn test_is_pid_alive() {
    assert!(mihomo::process::is_pid_alive(std::process::id()));
    assert!(!mihomo::process::is_pid_alive(u32::MAX));

    // 保持存活约 2 秒的子进程：Windows 用 ping 延迟，Unix 用 sleep
    #[cfg(windows)]
    let mut child = std::process::Command::new("cmd")
        .args(["/c", "ping 127.0.0.1 -n 3 >nul"])
        .spawn()
        .unwrap();
    #[cfg(not(windows))]
    let mut child = std::process::Command::new("sleep")
        .arg("2")
        .spawn()
        .unwrap();
    let pid = child.id();
    assert!(mihomo::process::is_pid_alive(pid));
    child.wait().unwrap();
    assert!(!mihomo::process::is_pid_alive(pid));
}

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
fn test_insert_sub_dedup() {
    let mut config = MihomoConfig::default_config();
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().join("config.yaml");
    config
        .insert_sub(
            "https://example.com/sub".to_string(),
            "订阅".to_string(),
            &path,
        )
        .unwrap();
    config
        .insert_sub(
            "https://example.com/sub".to_string(),
            "订阅".to_string(),
            &path,
        )
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
fn test_group_name_fallback() {
    let config = MihomoConfig::default_config();
    assert_eq!(config.group_name(), "Proxy");
    let mut config2 = MihomoConfig::default_config();
    config2.proxy_groups.clear();
    assert_eq!(config2.group_name(), "Proxy");
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
    let lines = crate::tui::windows::mihomo_log::read_tail(&path, 128 * 1024, 10);
    assert_eq!(lines.len(), 10);
    assert_eq!(lines[0], "line 90");
    assert_eq!(lines[9], "line 99");
}

#[test]
fn test_parse_env_file() {
    let map = mihomo::binary::parse_env_file(
        "# comment\n\nMIHOMO_EXE=/usr/bin/mihomo\nEMPTY=\nQUOTED=\"a b\"\nBADLINE\n",
    );
    assert_eq!(map.get("MIHOMO_EXE").unwrap(), "/usr/bin/mihomo");
    assert_eq!(map.get("QUOTED").unwrap(), "a b");
    assert_eq!(map.get("EMPTY").unwrap(), "");
    assert!(!map.contains_key("BADLINE"));
}

#[test]
fn test_resolve_env_var_wins() {
    let settings = Settings::default();
    let dir = tempfile::TempDir::new().unwrap();
    let r = mihomo::binary::resolve_mihomo_exe_with(
        &settings,
        Some("/from/env/mihomo".to_string()),
        Some(&dir.path().join(".env")),
        None,
    );
    assert_eq!(r.cmd, "/from/env/mihomo");
    assert_eq!(r.source, mihomo::BinarySource::EnvVar);
}

#[test]
fn test_resolve_env_file_wins_over_settings() {
    let settings = Settings {
        mihomo_exe: "C:\\explicit\\mihomo.exe".to_string(),
        ..Settings::default()
    };
    let dir = tempfile::TempDir::new().unwrap();
    let env_path = dir.path().join(".env");
    std::fs::write(&env_path, "MIHOMO_EXE=mihomo.exe\n").unwrap();
    let r = mihomo::binary::resolve_mihomo_exe_with(&settings, None, Some(&env_path), None);
    assert_eq!(r.source, mihomo::BinarySource::EnvFile);
    assert!(r.cmd.contains("mihomo.exe"));
}

#[test]
fn test_resolve_env_file_relative_joins_dir() {
    let settings = Settings::default();
    let dir = tempfile::TempDir::new().unwrap();
    let env_path = dir.path().join(".env");
    std::fs::write(&env_path, "MIHOMO_EXE=mihomo.exe\n").unwrap();
    let r = mihomo::binary::resolve_mihomo_exe_with(&settings, None, Some(&env_path), None);
    assert_eq!(
        r.cmd,
        dir.path().join("mihomo.exe").to_string_lossy().into_owned()
    );
    assert_eq!(r.source, mihomo::BinarySource::EnvFile);
}

#[test]
fn test_resolve_settings_explicit_path() {
    let settings = Settings {
        mihomo_exe: "/opt/bin/mihomo".to_string(),
        ..Settings::default()
    };
    let r = mihomo::binary::resolve_mihomo_exe_with(&settings, None, None, None);
    assert_eq!(r.cmd, "/opt/bin/mihomo");
    assert_eq!(r.source, mihomo::BinarySource::Settings);
}

#[test]
fn test_resolve_wrapper_precedes_path() {
    let settings = Settings::default();
    let r = mihomo::binary::resolve_mihomo_exe_with(
        &settings,
        None,
        None,
        Some(std::path::PathBuf::from("/run/wrappers/bin/mihomo")),
    );
    assert_eq!(r.source, mihomo::BinarySource::NixWrapper);
    assert_eq!(r.cmd, "/run/wrappers/bin/mihomo");
}

#[test]
fn test_resolve_fallback_to_path() {
    let settings = Settings::default();
    // 显式传入 wrapper=None，避免依赖宿主环境（如 NixOS 的 /run/wrappers）
    let r = mihomo::binary::resolve_mihomo_exe_with(&settings, None, None, None);
    assert_eq!(r.source, mihomo::BinarySource::Path);
    assert!(!r.cmd.is_empty());

    let settings2 = Settings {
        mihomo_exe: "my-mihomo".to_string(),
        ..Settings::default()
    };
    let r2 = mihomo::binary::resolve_mihomo_exe_with(&settings2, None, None, None);
    assert_eq!(r2.cmd, "my-mihomo");
    assert_eq!(r2.source, mihomo::BinarySource::Path);
}

#[cfg(windows)]
#[test]
fn test_find_on_path_windows() {
    let dir = tempfile::TempDir::new().unwrap();
    let exe = dir.path().join("mihomo-windows-amd64.exe");
    std::fs::write(&exe, b"x").unwrap();
    let path = std::env::join_paths([dir.path()]).unwrap();
    assert_eq!(
        mihomo::binary::find_on_path(&path, "mihomo-windows-amd64.exe"),
        Some(exe.to_string_lossy().into_owned())
    );
    assert_eq!(mihomo::binary::find_on_path(&path, "missing.exe"), None);
}

#[cfg(windows)]
#[test]
fn test_mihomo_windows_candidates_prefers_current_arch() {
    let names = mihomo::binary::mihomo_windows_candidates();
    let expected_arch = match std::env::consts::ARCH {
        "x86_64" => "amd64",
        "aarch64" => "arm64",
        "x86" => "386",
        other => other,
    };
    assert_eq!(names[0], format!("mihomo-windows-{expected_arch}.exe"));
    assert!(names.len() >= 3);
    for a in ["amd64", "arm64", "386"] {
        assert!(names.iter().any(|n| n.ends_with(&format!("{a}.exe"))));
    }
}
