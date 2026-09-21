//! mihomo 进程管理：内核来源物化、端口/状态探测、启动/停止/替换。
//!
//! 平台差异收敛在 `platform` 子模块（`windows`/`unix` 二选一，见各文件顶部文档）：
//! 平台只实现「内核可执行来源的物化与启动/替换」的最小差异（Linux memfd、
//! Windows 自删除临时文件），公共流程（端口检查、状态判定、日志重定向语义）
//! 保持在本模块，双平台行为一致。
//!
//! 内核可执行文件来自**内嵌**（`super::embedded`）：默认解压到内存执行，
//! 失败或显式要求时兜底释放到缓存目录。运行状态只看控制端口是否可达
//! （不区分是否由本程序启动）；停止时按控制端口找到属主 mihomo 并结束，
//! 因此外部启动的实例也能直接关闭，且不会误杀其它实例。
use crate::error::Error;
use crate::settings::Settings;
use std::ffi::OsString;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::Duration;

use super::embedded;

#[cfg(unix)]
mod unix;
#[cfg(windows)]
mod windows;

#[cfg(unix)]
use unix as platform;
/// 当前平台实现（二选一）
#[cfg(windows)]
use windows as platform;

// ===== 内核可执行来源 =====

/// 内嵌 mihomo 的运行形态（跨平台统一展示，日志/UI 用）
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BinarySource {
    /// Linux memfd：解压只在内存，运行期不落盘
    Memory,
    /// Windows 临时文件：进程退出后自动删除
    TempFile(PathBuf),
    /// 磁盘缓存文件：兜底、强制落盘或非 Linux 平台
    CacheFile(PathBuf),
}

impl std::fmt::Display for BinarySource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Memory => write!(f, "内存执行 (memfd)"),
            Self::TempFile(path) => write!(f, "临时文件: {}", path.display()),
            Self::CacheFile(path) => write!(f, "磁盘文件: {}", path.display()),
        }
    }
}

// ===== mihomo 运行状态 =====

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MihomoStatus {
    Stopped,
    /// 运行中；PID 为 0 表示未记录（端口探测发现，仅展示「运行中」）
    Running(u32),
}

// ===== 端口 / 进程探测 =====

pub fn is_port_up(settings: &Settings) -> bool {
    ctrl_addr_up(&settings.mihomo_ctrl_addr)
}

/// 从 external-controller 地址取控制端口（兼容 `127.0.0.1:9090` 与 `:9090`）
pub fn ctrl_port(addr: &str) -> Option<u16> {
    addr.rsplit_once(':').and_then(|(_, p)| p.parse().ok())
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

/// 当前状态只看控制端口：可达 = 运行中（PID 未知，置 0），否则停止。
pub fn detect_status(settings: &Settings) -> MihomoStatus {
    if is_port_up(settings) {
        MihomoStatus::Running(0)
    } else {
        MihomoStatus::Stopped
    }
}

// ===== mihomo 进程管理 =====

/// 启动内嵌 mihomo。`elevate` 仅 Windows 生效（UAC 提权启动）。
/// 返回 (PID, 实际内核来源)。
#[cfg_attr(not(windows), allow(unused_variables))]
pub fn start_mihomo(
    settings: &Settings,
    config_path: &Path,
    elevate: bool,
) -> Result<(u32, BinarySource), Error> {
    let config_dir = config_path
        .parent()
        .ok_or(Error::Process("无法获取配置目录".to_string()))?;
    if is_port_up(settings) {
        return Err(Error::Process(
            "端口已被 mihomo 占用，未启动新进程".to_string(),
        ));
    }

    // 平台层物化内核（内存 / 自删除临时文件 / 缓存兜底）并启动
    let prepared = platform::prepare_exec()?;
    platform::start(prepared, config_dir, elevate)
}

/// 以「原生 mihomo」方式运行：参数原样透传并替换/接管当前进程（`coclash core`）。
/// Unix 成功时进程已被替换（不返回）；Windows 等待子进程结束并透传退出码。
pub fn exec_core(args: &[OsString]) -> Result<(), Error> {
    let prepared = platform::prepare_exec()?;
    platform::exec_replace(&prepared, args)
}

/// 停止 mihomo：按控制端口找到属主进程（不区分是否由本程序启动），找不到则报错
pub fn stop_mihomo(settings: &Settings) -> Result<(), Error> {
    let pid = platform::find_mihomo_pid(settings)
        .ok_or_else(|| Error::Process("未找到监听控制端口的 mihomo 进程".to_string()))?;
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

    #[test]
    fn test_binary_source_display() {
        assert_eq!(BinarySource::Memory.to_string(), "内存执行 (memfd)");
        let temp = BinarySource::TempFile(PathBuf::from("/tmp/mihomo.exe"));
        assert!(temp.to_string().contains("临时文件"));
        let cache = BinarySource::CacheFile(PathBuf::from("/cache/mihomo"));
        assert!(cache.to_string().contains("磁盘文件"));
    }
}
