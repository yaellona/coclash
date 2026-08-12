//! mihomo 进程管理：pidfile、端口/状态探测、启动/停止。
//!
//! 平台差异收敛在 `platform` 子模块（`windows`/`unix` 二选一，见各文件顶部文档）：
//! 各平台只实现「探测与启动/终止」的最小差异，公共流程（pidfile、状态判定、
//! 日志重定向、SIGTERM→SIGKILL 语义）保持在本模块，双平台行为一致。
use crate::constants::{MIHOMO_LOG_FILE, PID_FILE};
use crate::error::Error;
use crate::settings::Settings;
use std::fs::{self, OpenOptions};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use super::binary::{ResolvedBinary, resolve_mihomo_exe};

#[cfg(windows)]
mod windows;
#[cfg(unix)]
mod unix;

/// 当前平台实现（二选一）
#[cfg(windows)]
use windows as platform;
#[cfg(unix)]
use unix as platform;

// ===== mihomo 运行状态 =====

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MihomoStatus {
    Stopped,
    /// 由本程序启动，记录 PID
    RunningByUs(u32),
    /// 端口被占用但无 PID 记录（外部启动的实例）
    External,
}

// ===== pidfile =====

/// 供平台子模块写入（Windows 提权标记格式 `{pid}:1`）
pub(super) fn pidfile_path(config_dir: &Path) -> PathBuf {
    config_dir.join(PID_FILE)
}

pub(crate) fn save_pid(config_dir: &Path, pid: u32) -> Result<(), Error> {
    fs::write(pidfile_path(config_dir), pid.to_string())
        .map_err(|e| Error::Process(format!("写入 PID 文件失败: {e}")))
}

pub(crate) fn load_pidfile(config_dir: &Path) -> Option<u32> {
    load_pidfile_elevated(config_dir).map(|(pid, _)| pid)
}

/// 兼容旧格式纯数字 pidfile；返回 (pid, 是否提权)
pub(crate) fn load_pidfile_elevated(config_dir: &Path) -> Option<(u32, bool)> {
    let content = fs::read_to_string(pidfile_path(config_dir)).ok()?;
    let content = content.trim();
    if let Some((pid, flag)) = content.split_once(':') {
        let pid: u32 = pid.trim().parse().ok()?;
        return Some((pid, flag.trim() == "1"));
    }
    let pid: u32 = content.parse().ok()?;
    Some((pid, false))
}

pub(crate) fn clear_pidfile(config_dir: &Path) {
    let _ = fs::remove_file(pidfile_path(config_dir));
}

// ===== 端口 / 进程探测 =====

pub fn is_port_up(settings: &Settings) -> bool {
    ctrl_addr_up(&settings.mihomo_ctrl_addr)
}

pub fn ctrl_addr_up(addr: &str) -> bool {
    let addr: SocketAddr = match addr.parse() {
        Ok(a) => a,
        Err(_) => return false,
    };
    std::net::TcpStream::connect_timeout(&addr, Duration::from_millis(300)).is_ok()
}

pub async fn ctrl_addr_up_async(addr: &str) -> bool {
    let addr: SocketAddr = match addr.parse() {
        Ok(a) => a,
        Err(_) => return false,
    };
    tokio::time::timeout(
        Duration::from_millis(300),
        tokio::net::TcpStream::connect(addr),
    )
    .await
    .is_ok_and(|r| r.is_ok())
}

pub fn is_pid_alive(pid: u32) -> bool {
    platform::is_pid_alive(pid)
}

/// PID 存活且确实是本程序启动的那个 mihomo（防止 pidfile 过期后 PID 被复用误杀）
fn is_mihomo_pid(binary: &ResolvedBinary, config_dir: &Path, pid: u32) -> bool {
    is_pid_alive(pid) && platform::is_mihomo_pid(binary, config_dir, pid)
}

/// 综合端口与 PID 记录判断当前状态，同时清理过期的 pidfile
pub fn detect_status(settings: &Settings, config_dir: &Path) -> MihomoStatus {
    let binary = resolve_mihomo_exe(settings);
    if let Some(pid) = load_pidfile(config_dir) {
        if is_mihomo_pid(&binary, config_dir, pid) {
            return MihomoStatus::RunningByUs(pid);
        }
        clear_pidfile(config_dir);
    }
    if is_port_up(settings) {
        MihomoStatus::External
    } else {
        MihomoStatus::Stopped
    }
}

// ===== mihomo 进程管理 =====

/// 启动 mihomo。`elevate` 仅 Windows 生效（UAC 提权启动）。
#[cfg_attr(not(windows), allow(unused_variables))]
pub fn start_mihomo(
    settings: &Settings,
    config_path: &Path,
    elevate: bool,
) -> Result<(u32, ResolvedBinary), Error> {
    let config_dir = config_path
        .parent()
        .ok_or(Error::Process("无法获取配置目录".to_string()))?;
    if is_port_up(settings) {
        return Err(Error::Process(
            "端口已被 mihomo 占用，未启动新进程".to_string(),
        ));
    }

    let binary = resolve_mihomo_exe(settings);

    #[cfg(windows)]
    if elevate {
        let pid = platform::start_elevated(&binary, config_dir)?;
        return Ok((pid, binary));
    }

    let pid = spawn_detached(&binary, config_dir)?;
    save_pid(config_dir, pid)?;
    Ok((pid, binary))
}

/// 普通（非提权）启动：日志重定向 + 平台分离会话配置
fn spawn_detached(binary: &ResolvedBinary, config_dir: &Path) -> Result<u32, Error> {
    let mut cmd = Command::new(&binary.cmd);
    cmd.args([
        "-d",
        config_dir
            .to_str()
            .ok_or(Error::Process("config路径无效".to_string()))?,
    ])
    .stdin(Stdio::null());

    // mihomo 的 stdout/stderr 重定向到日志文件，便于排查启动失败
    let log_path = config_dir.join(MIHOMO_LOG_FILE);
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .map_err(|e| Error::Process(format!("打开日志文件失败: {e}")))?;
    let file2 = file
        .try_clone()
        .map_err(|e| Error::Process(format!("克隆日志文件失败: {e}")))?;
    cmd.stdout(Stdio::from(file)).stderr(Stdio::from(file2));

    platform::configure_detached(&mut cmd)?;
    let child = cmd
        .spawn()
        .map_err(|e| Error::Process(format!("启动 mihomo 失败: {e}")))?;
    Ok(child.id())
}

/// 只停止由本程序启动的 mihomo（依据 pidfile）；外部实例拒绝操作
pub fn stop_mihomo(settings: &Settings, config_dir: &Path) -> Result<(), Error> {
    let (pid, elevated) = match load_pidfile_elevated(config_dir) {
        Some(p) => p,
        None => {
            return Err(Error::Process(
                "未找到由本程序启动的 mihomo（无 PID 记录）；若是外部启动的实例，请自行关闭"
                    .to_string(),
            ));
        }
    };
    let binary = resolve_mihomo_exe(settings);
    if !is_mihomo_pid(&binary, config_dir, pid) {
        clear_pidfile(config_dir);
        return Err(Error::Process(format!(
            "PID 记录 ({pid}) 已失效（进程不存在或不是 mihomo），已清除记录"
        )));
    }
    kill_pid(pid, elevated)?;
    clear_pidfile(config_dir);
    Ok(())
}

fn kill_pid(pid: u32, elevated: bool) -> Result<(), Error> {
    platform::kill_pid(pid, elevated)
}

/// TUN 权限检查（仅 Unix；Windows 无此概念）
#[cfg(unix)]
pub use unix::tun_capability_warning;

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_pidfile_roundtrip() {
        let dir = TempDir::new().unwrap();
        save_pid(dir.path(), 12345).unwrap();
        assert_eq!(load_pidfile(dir.path()), Some(12345));
        clear_pidfile(dir.path());
        assert_eq!(load_pidfile(dir.path()), None);
    }

    #[test]
    fn test_pidfile_invalid_content() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join(PID_FILE), "not-a-pid").unwrap();
        assert_eq!(load_pidfile(dir.path()), None);
    }

    #[test]
    fn test_pidfile_elevated_roundtrip() {
        let dir = TempDir::new().unwrap();
        save_pid(dir.path(), 12345).unwrap();
        assert_eq!(load_pidfile(dir.path()), Some(12345));
        assert_eq!(load_pidfile_elevated(dir.path()), Some((12345, false)));
        std::fs::write(dir.path().join(PID_FILE), "12345:1").unwrap();
        assert_eq!(load_pidfile_elevated(dir.path()), Some((12345, true)));
        clear_pidfile(dir.path());
        assert_eq!(load_pidfile_elevated(dir.path()), None);
    }

    #[test]
    fn test_is_pid_alive() {
        assert!(is_pid_alive(std::process::id()));
        assert!(!is_pid_alive(u32::MAX));

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
        assert!(is_pid_alive(pid));
        child.wait().unwrap();
        assert!(!is_pid_alive(pid));
    }
}
