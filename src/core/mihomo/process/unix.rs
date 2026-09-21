//! Unix 平台实现：setsid 分离会话、SIGTERM→SIGKILL 信号终止、/proc 进程与端口探测。
use crate::error::Error;
use crate::settings::Settings;
use std::fs;
use std::process::Command;
use std::time::Duration;

/// 分离会话（setsid），使 mihomo 在终端退出后继续运行
pub(super) fn configure_detached(cmd: &mut Command) -> Result<(), Error> {
    use std::os::unix::process::CommandExt;
    unsafe {
        cmd.pre_exec(|| {
            if libc::setsid() < 0 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
    Ok(())
}

pub(super) fn is_pid_alive(pid: u32) -> bool {
    // pid 0 和超出 pid_t(i32) 范围的 pid 是非法 pid；u32::MAX 会溢出成 -1，
    // 使 kill(-1, 0) 探测"全部进程"而非单个进程，导致误判存活。
    if pid == 0 || pid > i32::MAX as u32 {
        return false;
    }
    unsafe { libc::kill(pid as i32, 0) == 0 }
}

/// 找到监听控制端口的 mihomo 进程（不区分是否由本程序启动）
pub(super) fn find_mihomo_pid(settings: &Settings) -> Option<u32> {
    let port = super::ctrl_port(&settings.mihomo_ctrl_addr)?;
    let inode = listening_socket_inode(port)?;
    find_mihomo_pid_by_inode(&inode)
}

/// 从 `/proc/net/tcp{,6}` 找 LISTEN（状态 0A）且本地端口匹配的 socket inode
fn listening_socket_inode(port: u16) -> Option<String> {
    for path in ["/proc/net/tcp", "/proc/net/tcp6"] {
        let Ok(content) = fs::read_to_string(path) else {
            continue;
        };
        for line in content.lines().skip(1) {
            let cols: Vec<&str> = line.split_whitespace().collect();
            if cols.len() < 10 || cols[3] != "0A" {
                continue;
            }
            let Some((_, hex_port)) = cols[1].rsplit_once(':') else {
                continue;
            };
            if u16::from_str_radix(hex_port, 16).ok() == Some(port) {
                return Some(cols[9].to_string());
            }
        }
    }
    None
}

/// 在 `comm == "mihomo"` 的进程里找持有该 socket inode 的 PID
fn find_mihomo_pid_by_inode(inode: &str) -> Option<u32> {
    let needle = format!("socket:[{inode}]");
    for entry in fs::read_dir("/proc").ok()?.flatten() {
        let pid: u32 = match entry.file_name().to_string_lossy().parse() {
            Ok(p) => p,
            Err(_) => continue,
        };
        let base = entry.path();
        let comm = match fs::read_to_string(base.join("comm")) {
            Ok(c) => c,
            Err(_) => continue,
        };
        if comm.trim() != "mihomo" {
            continue;
        }
        let Ok(fds) = fs::read_dir(base.join("fd")) else {
            continue;
        };
        for fd in fds.flatten() {
            if let Ok(target) = fs::read_link(fd.path())
                && target.to_string_lossy() == needle
            {
                return Some(pid);
            }
        }
    }
    None
}

pub(super) fn kill_pid(pid: u32) -> Result<(), Error> {
    unsafe {
        libc::kill(pid as i32, libc::SIGTERM);
    }
    let deadline = std::time::Instant::now() + Duration::from_secs(3);
    while super::is_pid_alive(pid) && std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(100));
    }
    if super::is_pid_alive(pid) {
        unsafe {
            libc::kill(pid as i32, libc::SIGKILL);
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    // waitpid 收割，防止僵尸进程
    unsafe {
        let mut status = 0;
        libc::waitpid(pid as i32, &mut status, 0);
    }
    Ok(())
}

// ===== TUN 权限检查 =====

/// 任意一个 mihomo 进程（用于 TUN capabilities 检查）
fn find_any_mihomo_pid() -> Option<u32> {
    for entry in fs::read_dir("/proc").ok()?.flatten() {
        let pid: u32 = match entry.file_name().to_string_lossy().parse() {
            Ok(p) => p,
            Err(_) => continue,
        };
        let comm = match fs::read_to_string(entry.path().join("comm")) {
            Ok(c) => c,
            Err(_) => continue,
        };
        if comm.trim() == "mihomo" {
            return Some(pid);
        }
    }
    None
}

/// TUN 权限检查：mihomo 进程缺少 CAP_NET_ADMIN/CAP_NET_RAW 时给出提示。
/// 内嵌 mihomo 释放到用户缓存目录，不带文件 capabilities，需要用户手动 setcap。
pub fn tun_capability_warning() -> Option<String> {
    let pid = find_any_mihomo_pid()?;
    let status = fs::read_to_string(format!("/proc/{pid}/status")).ok()?;
    let cap_eff = status.lines().find_map(|l| {
        l.strip_prefix("CapEff:\t")
            .and_then(|v| u64::from_str_radix(v.trim(), 16).ok())
    })?;
    const CAP_NET_ADMIN: u64 = 1 << 12;
    const CAP_NET_RAW: u64 = 1 << 13;
    if cap_eff & (CAP_NET_ADMIN | CAP_NET_RAW) == (CAP_NET_ADMIN | CAP_NET_RAW) {
        return None;
    }
    let hint = match crate::core::mihomo::embedded::ensure_extracted() {
        Ok(bin) => format!(
            "请执行一次: sudo setcap cap_net_admin,cap_net_raw+eip {}",
            bin.display()
        ),
        Err(_) => "请用 setcap 给 mihomo 授予 CAP_NET_ADMIN/CAP_NET_RAW".to_string(),
    };
    Some(format!(
        "mihomo(PID={pid})缺少CAP_NET_ADMIN/CAP_NET_RAW，TUN可能起不来。{hint}"
    ))
}
