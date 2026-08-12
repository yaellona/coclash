//! mihomo 可执行文件解析链：
//! 环境变量 > 同目录 .env > settings 显式路径 > NixOS wrapper > PATH。
use crate::settings::Settings;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

pub const ENV_VAR_NAME: &str = "COCLASH_MIHOMO_EXE";
pub const ENV_FILE_KEY: &str = "MIHOMO_EXE";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinarySource {
    EnvVar,
    EnvFile,
    Settings,
    #[cfg_attr(windows, allow(dead_code))]
    NixWrapper,
    Path,
}

impl BinarySource {
    pub fn label(self) -> &'static str {
        match self {
            BinarySource::EnvVar => "环境变量",
            BinarySource::EnvFile => "同目录 .env",
            BinarySource::Settings => "settings.json",
            BinarySource::NixWrapper => "NixOS wrapper",
            BinarySource::Path => "PATH",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedBinary {
    pub cmd: String,
    pub source: BinarySource,
}

/// 解析 .env 内容（支持 # 注释、空行、可选的引号包裹）
pub(crate) fn parse_env_file(content: &str) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let Some((k, v)) = line.split_once('=') else {
            continue;
        };
        let v = v.trim().trim_matches('"').trim_matches('\'');
        map.insert(k.trim().to_string(), v.to_string());
    }
    map
}

/// NixOS setcap wrapper（独立于 shell PATH，任何启动方式都能命中）
#[cfg(unix)]
fn nix_wrapper_path() -> Option<PathBuf> {
    let p = Path::new("/run/wrappers/bin/mihomo");
    if p.exists() {
        Some(p.to_path_buf())
    } else {
        None
    }
}

#[cfg(not(unix))]
fn nix_wrapper_path() -> Option<PathBuf> {
    None
}

pub fn resolve_mihomo_exe(settings: &Settings) -> ResolvedBinary {
    let env_val = std::env::var(ENV_VAR_NAME).ok();
    let env_file = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.join(".env")));
    let mut resolved =
        resolve_mihomo_exe_with(settings, env_val, env_file.as_deref(), nix_wrapper_path());
    if resolved.source == BinarySource::Path {
        upgrade_path_fallback(&mut resolved);
    }
    resolved
}

/// PATH 兜底命中后升级为完整路径；若默认名不存在则尝试 winget 的
/// MetaCubeX.Mihomo 包实际提供的 `mihomo-windows-{arch}.exe`。
#[cfg(windows)]
fn upgrade_path_fallback(resolved: &mut ResolvedBinary) {
    if let Some(path_var) = std::env::var_os("PATH") {
        if let Some(found) = find_on_path(&path_var, &resolved.cmd) {
            resolved.cmd = found;
            return;
        }
        for candidate in mihomo_windows_candidates() {
            if let Some(found) = find_on_path(&path_var, &candidate) {
                resolved.cmd = found;
                return;
            }
        }
    }
}

#[cfg(not(windows))]
fn upgrade_path_fallback(_resolved: &mut ResolvedBinary) {}

/// 按 PATH 逐目录查找可执行文件，命中返回完整路径
#[cfg(windows)]
pub(crate) fn find_on_path(path_var: &std::ffi::OsStr, name: &str) -> Option<String> {
    for dir in std::env::split_paths(path_var) {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate.to_string_lossy().into_owned());
        }
    }
    None
}

/// winget MetaCubeX.Mihomo 包解压后的可执行文件名（优先当前架构）
#[cfg(windows)]
pub(crate) fn mihomo_windows_candidates() -> Vec<String> {
    let arch = match std::env::consts::ARCH {
        "x86_64" => "amd64",
        "aarch64" => "arm64",
        "x86" => "386",
        other => other,
    };
    let mut names = vec![format!("mihomo-windows-{arch}.exe")];
    for a in ["amd64", "arm64", "386"] {
        let n = format!("mihomo-windows-{a}.exe");
        if !names.contains(&n) {
            names.push(n);
        }
    }
    names
}

/// 解析链主体；`wrapper` 由调用方探测传入，便于测试隔离宿主环境。
pub(crate) fn resolve_mihomo_exe_with(
    settings: &Settings,
    env_val: Option<String>,
    env_file: Option<&Path>,
    wrapper: Option<PathBuf>,
) -> ResolvedBinary {
    // 1. 环境变量（nix 模块 makeWrapper 注入）
    if let Some(v) = env_val {
        let v = v.trim().to_string();
        if !v.is_empty() {
            return ResolvedBinary {
                cmd: v,
                source: BinarySource::EnvVar,
            };
        }
    }

    // 2. 二进制同目录 .env 的 MIHOMO_EXE；相对值以 .env 所在目录为基准
    if let Some(path) = env_file
        && let Ok(content) = fs::read_to_string(path)
        && let Some(v) = parse_env_file(&content).get(ENV_FILE_KEY)
    {
        let v = v.trim().to_string();
        if !v.is_empty() {
            let cmd = if Path::new(&v).is_absolute() {
                v
            } else {
                path.parent()
                    .unwrap_or(Path::new("."))
                    .join(&v)
                    .to_string_lossy()
                    .into_owned()
            };
            return ResolvedBinary {
                cmd,
                source: BinarySource::EnvFile,
            };
        }
    }

    // 3. settings.json 显式路径（含分隔符，避免旧默认文件名遮蔽其他来源）
    let settings_val = settings.mihomo_exe.trim().to_string();
    if !settings_val.is_empty()
        && (settings_val.contains('/')
            || settings_val.contains('\\')
            || Path::new(&settings_val).is_absolute())
    {
        return ResolvedBinary {
            cmd: settings_val,
            source: BinarySource::Settings,
        };
    }

    // 4. NixOS setcap wrapper
    if let Some(w) = wrapper {
        return ResolvedBinary {
            cmd: w.to_string_lossy().into_owned(),
            source: BinarySource::NixWrapper,
        };
    }

    // 5. PATH 兜底
    let name = if settings_val.is_empty() {
        default_mihomo_name().to_string()
    } else {
        settings_val
    };
    ResolvedBinary {
        cmd: name,
        source: BinarySource::Path,
    }
}

#[cfg(windows)]
fn default_mihomo_name() -> &'static str {
    "mihomo.exe"
}

#[cfg(not(windows))]
fn default_mihomo_name() -> &'static str {
    "mihomo"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_env_file() {
        let map = parse_env_file(
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
        let r = resolve_mihomo_exe_with(
            &settings,
            Some("/from/env/mihomo".to_string()),
            Some(&dir.path().join(".env")),
            None,
        );
        assert_eq!(r.cmd, "/from/env/mihomo");
        assert_eq!(r.source, BinarySource::EnvVar);
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
        let r = resolve_mihomo_exe_with(&settings, None, Some(&env_path), None);
        assert_eq!(r.source, BinarySource::EnvFile);
        assert!(r.cmd.contains("mihomo.exe"));
    }

    #[test]
    fn test_resolve_env_file_relative_joins_dir() {
        let settings = Settings::default();
        let dir = tempfile::TempDir::new().unwrap();
        let env_path = dir.path().join(".env");
        std::fs::write(&env_path, "MIHOMO_EXE=mihomo.exe\n").unwrap();
        let r = resolve_mihomo_exe_with(&settings, None, Some(&env_path), None);
        assert_eq!(
            r.cmd,
            dir.path().join("mihomo.exe").to_string_lossy().into_owned()
        );
        assert_eq!(r.source, BinarySource::EnvFile);
    }

    #[test]
    fn test_resolve_settings_explicit_path() {
        let settings = Settings {
            mihomo_exe: "/opt/bin/mihomo".to_string(),
            ..Settings::default()
        };
        let r = resolve_mihomo_exe_with(&settings, None, None, None);
        assert_eq!(r.cmd, "/opt/bin/mihomo");
        assert_eq!(r.source, BinarySource::Settings);
    }

    #[test]
    fn test_resolve_wrapper_precedes_path() {
        let settings = Settings::default();
        let r = resolve_mihomo_exe_with(
            &settings,
            None,
            None,
            Some(std::path::PathBuf::from("/run/wrappers/bin/mihomo")),
        );
        assert_eq!(r.source, BinarySource::NixWrapper);
        assert_eq!(r.cmd, "/run/wrappers/bin/mihomo");
    }

    #[test]
    fn test_resolve_fallback_to_path() {
        let settings = Settings::default();
        // 显式传入 wrapper=None，避免依赖宿主环境（如 NixOS 的 /run/wrappers）
        let r = resolve_mihomo_exe_with(&settings, None, None, None);
        assert_eq!(r.source, BinarySource::Path);
        assert!(!r.cmd.is_empty());

        let settings2 = Settings {
            mihomo_exe: "my-mihomo".to_string(),
            ..Settings::default()
        };
        let r2 = resolve_mihomo_exe_with(&settings2, None, None, None);
        assert_eq!(r2.cmd, "my-mihomo");
        assert_eq!(r2.source, BinarySource::Path);
    }

    #[cfg(windows)]
    #[test]
    fn test_find_on_path_windows() {
        let dir = tempfile::TempDir::new().unwrap();
        let exe = dir.path().join("mihomo-windows-amd64.exe");
        std::fs::write(&exe, b"x").unwrap();
        let path = std::env::join_paths([dir.path()]).unwrap();
        assert_eq!(
            find_on_path(&path, "mihomo-windows-amd64.exe"),
            Some(exe.to_string_lossy().into_owned())
        );
        assert_eq!(find_on_path(&path, "missing.exe"), None);
    }

    #[cfg(windows)]
    #[test]
    fn test_mihomo_windows_candidates_prefers_current_arch() {
        let names = mihomo_windows_candidates();
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
}
