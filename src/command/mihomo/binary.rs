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
    resolve_mihomo_exe_with(settings, env_val, env_file.as_deref(), nix_wrapper_path())
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
