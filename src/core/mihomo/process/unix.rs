//! Unix 平台实现：setsid 分离会话、SIGTERM→SIGKILL 信号终止、/proc 进程探测。
use super::super::binary::ResolvedBinary;
use crate::error::Error;
use std::fs;
use std::path::Path;
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

/// 判定依据：命令行包含 config_dir（与 windows 语义一致）
pub(super) fn is_mihomo_pid(_binary: &ResolvedBinary, config_dir: &Path, pid: u32) -> bool {
    fs::read(format!("/proc/{pid}/cmdline"))
        .map(|c| {
            let cmdline = String::from_utf8_lossy(&c);
            cmdline.contains(config_dir.to_str().unwrap_or(""))
        })
        .unwrap_or(false)
}

pub(super) fn kill_pid(pid: u32, _elevated: bool) -> Result<(), Error> {
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

// ===== 查找 mihomo PID（TUN 权限检查用）=====

fn find_mihomo_pid(config_dir: &str) -> Option<u32> {
    for entry in fs::read_dir("/proc").ok()?.flatten() {
        let pid: u32 = match entry.file_name().to_string_lossy().parse() {
            Ok(p) => p,
            Err(_) => continue,
        };
        let base = entry.path();
        let comm = match fs::read_to_string(base.join("comm")) {
            Ok(c) => c.trim().to_string(),
            Err(_) => continue,
        };
        if comm != "mihomo" {
            continue;
        }
        if config_dir.is_empty() {
            return Some(pid);
        }
        if let Ok(cmdline) = fs::read(base.join("cmdline"))
            && String::from_utf8_lossy(&cmdline).contains(config_dir)
        {
            return Some(pid);
        }
    }
    None
}

/// TUN 权限检查：mihomo 进程缺少 CAP_NET_ADMIN/CAP_NET_RAW 时给出提示
pub fn tun_capability_warning() -> Option<String> {
    let pid = find_mihomo_pid("")?;
    let status = fs::read_to_string(format!("/proc/{pid}/status")).ok()?;
    let cap_eff = status.lines().find_map(|l| {
        l.strip_prefix("CapEff:\t")
            .and_then(|v| u64::from_str_radix(v.trim(), 16).ok())
    })?;
    const CAP_NET_ADMIN: u64 = 1 << 12;
    const CAP_NET_RAW: u64 = 1 << 13;
    if cap_eff & (CAP_NET_ADMIN | CAP_NET_RAW) == (CAP_NET_ADMIN | CAP_NET_RAW) {
        None
    } else {
        Some(format!(
            "mihomo(PID={pid})缺少CAP_NET_ADMIN/CAP_NET_RAW，TUN可能起不来。请用NixOS security.wrappers或setcap授权"
        ))
    }
}
