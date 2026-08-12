//! mihomo 进程管理：进程表扫描、端口/状态探测、启动/停止。
//!
//! 平台差异收敛在 `platform` 子模块（`windows`/`unix` 二选一，见各文件顶部文档）：
//! 各平台只实现「探测与启动/终止」的最小差异，公共流程（进程表扫描、状态判定、
//! 日志重定向、SIGTERM→SIGKILL 语义）保持在本模块，双平台行为一致。
//!
//! 运行状态判定不依赖任何文件记录：通过扫描系统进程表找出命令行含本程序
//! `config_dir` 的 mihomo 进程，即为 RunningByUs；仅端口可达则为 External。
use crate::constants::MIHOMO_LOG_FILE;
use crate::error::Error;
use crate::settings::Settings;
use std::fs::OpenOptions;
use std::net::SocketAddr;
use std::path::Path;
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
    /// 运行中且命令行包含本程序 config_dir（判定方式：进程表扫描，无文件记录）
    RunningByUs(u32),
    /// 端口被占用但未找到匹配本 config_dir 的 mihomo 进程（外部启动的实例）
    External,
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

/// 扫描进程表判断当前状态：命令行含本程序 config_dir 的 mihomo 进程 → RunningByUs；
/// 仅端口可达 → External；否则 Stopped。不依赖任何文件记录。
pub fn detect_status(settings: &Settings, config_dir: &Path) -> MihomoStatus {
    let binary = resolve_mihomo_exe(settings);
    if let Some(pid) = platform::find_mihomo_pid(&binary, config_dir) {
        return MihomoStatus::RunningByUs(pid);
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

/// 只停止匹配本 config_dir 的 mihomo 进程（进程表扫描定位）；外部实例（命令行不含
/// config_dir）拒绝操作
pub fn stop_mihomo(settings: &Settings, config_dir: &Path) -> Result<(), Error> {
    let binary = resolve_mihomo_exe(settings);
    let pid = platform::find_mihomo_pid(&binary, config_dir).ok_or_else(|| {
        Error::Process(
            "未找到运行中的 mihomo（进程不存在或不是本配置目录启动的）；若是外部启动的实例，请自行关闭"
                .to_string(),
        )
    })?;
    kill_pid(pid)
}

fn kill_pid(pid: u32) -> Result<(), Error> {
    platform::kill_pid(pid)
}

/// TUN 权限检查（仅 Unix；Windows 无此概念）
#[cfg(unix)]
pub use unix::tun_capability_warning;

#[cfg(test)]
mod tests {
    use super::*;

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
