//! mihomo 进程管理：pidfile、端口/状态探测、启动/停止。
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
use std::sync::{Mutex, OnceLock};
#[cfg(windows)]
use windows_sys::Win32::Foundation::{
    CloseHandle, DuplicateHandle, ERROR_CANCELLED, GetLastError, HANDLE,
};
#[cfg(windows)]
use windows_sys::Win32::System::Threading::{
    GetCurrentProcess, GetProcessId, PROCESS_QUERY_LIMITED_INFORMATION, TerminateProcess,
};
#[cfg(windows)]
use windows_sys::Win32::UI::Shell::{
    SEE_MASK_FLAG_NO_UI, SEE_MASK_NOASYNC, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW,
    SHELLEXECUTEINFOW_0, ShellExecuteExW,
};
#[cfg(windows)]
use windows_sys::Win32::UI::WindowsAndMessaging::SW_HIDE;

/// 进程句柄在进程间共享是安全的（句柄值本身可被多个线程使用）
#[cfg(windows)]
#[derive(Clone, Copy)]
struct ProcessHandle(HANDLE);
#[cfg(windows)]
unsafe impl Send for ProcessHandle {}
#[cfg(windows)]
unsafe impl Sync for ProcessHandle {}

/// 提权启动的 mihomo 进程句柄缓存（本进程生命周期内有效）
#[cfg(windows)]
static ELEVATED_PROCESS: OnceLock<Mutex<Option<(u32, ProcessHandle)>>> = OnceLock::new();

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

fn pidfile_path(config_dir: &Path) -> PathBuf {
    config_dir.join(PID_FILE)
}

pub(crate) fn save_pid(config_dir: &Path, pid: u32) -> Result<(), Error> {
    fs::write(pidfile_path(config_dir), pid.to_string())
        .map_err(|e| Error::Process(format!("写入 PID 文件失败: {e}")))
}

/// 记录提权启动的进程，pidfile 写入 `{pid}:1` 标记
#[cfg(windows)]
pub(crate) fn save_pid_elevated(config_dir: &Path, pid: u32) -> Result<(), Error> {
    fs::write(pidfile_path(config_dir), format!("{pid}:1"))
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
    #[cfg(windows)]
    {
        windows_image_name(pid).is_some()
    }
    #[cfg(unix)]
    {
        // pid 0 和超出 pid_t(i32) 范围的 pid 是非法 pid；u32::MAX 会溢出成 -1，
        // 使 kill(-1, 0) 探测"全部进程"而非单个进程，导致误判存活。
        if pid == 0 || pid > i32::MAX as u32 {
            return false;
        }
        unsafe { libc::kill(pid as i32, 0) == 0 }
    }
}

/// PID 存活且确实是本程序启动的那个 mihomo（防止 pidfile 过期后 PID 被复用误杀）
fn is_mihomo_pid(binary: &ResolvedBinary, config_dir: &Path, pid: u32) -> bool {
    if !is_pid_alive(pid) {
        return false;
    }
    #[cfg(windows)]
    {
        if let Some((name, cmdline)) = windows_process_info(pid) {
            matches_mihomo_info(&name, &cmdline, binary, config_dir)
        } else {
            // CIM 查询失败时退回镜像名比对
            windows_image_name(pid)
                .map(|img| img.eq_ignore_ascii_case(&mihomo_image_name(&binary.cmd)))
                .unwrap_or(false)
        }
    }
    #[cfg(unix)]
    {
        let _ = binary;
        fs::read(format!("/proc/{pid}/cmdline"))
            .map(|c| {
                let cmdline = String::from_utf8_lossy(&c);
                cmdline.contains(config_dir.to_str().unwrap_or(""))
            })
            .unwrap_or(false)
    }
}

/// 判断依据：命令行包含 config_dir 为主（与 unix 语义一致，不依赖解析到的 mihomo 文件名，
/// 避免不同解析结果互相误判为外部并清掉 pidfile）；镜像名比对为辅助兜底。
#[cfg(windows)]
fn matches_mihomo_info(
    name: &str,
    cmdline: &str,
    binary: &ResolvedBinary,
    config_dir: &Path,
) -> bool {
    match config_dir.to_str() {
        Some(dir) if !dir.is_empty() && cmdline.contains(dir) => true,
        _ => name.eq_ignore_ascii_case(&mihomo_image_name(&binary.cmd)),
    }
}

/// 一次调用同时取进程镜像名与命令行（PowerShell 5.1 CIM）
#[cfg(windows)]
fn windows_process_info(pid: u32) -> Option<(String, String)> {
    let script = format!(
        "Get-CimInstance Win32_Process -Filter 'ProcessId={pid}' | Select-Object -Property Name,CommandLine | ConvertTo-Json -Compress"
    );
    let output = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let value: serde_json::Value = serde_json::from_str(text.trim()).ok()?;
    let name = value.get("Name")?.as_str()?.to_string();
    let cmdline = value
        .get("CommandLine")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    Some((name, cmdline))
}

#[cfg(windows)]
fn windows_image_name(pid: u32) -> Option<String> {
    let output = Command::new("tasklist")
        .args(["/FI", &format!("PID eq {pid}"), "/FO", "CSV", "/NH"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .output()
        .ok()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let line = stdout.lines().next()?;
    if !line.contains(&pid.to_string()) {
        return None;
    }
    line.trim_matches('"')
        .split("\",\"")
        .next()
        .map(|s| s.to_string())
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

#[cfg(windows)]
fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// 通过 UAC 提示以管理员权限执行程序，返回 hProcess 句柄
#[cfg(windows)]
fn shell_run_elevated(file: &str, params: &str) -> Result<ProcessHandle, Error> {
    // 宽字符串须在 ShellExecuteExW 调用期间保持存活
    let verb = to_wide("runas");
    let file_w = to_wide(file);
    let params_w = to_wide(params);
    let mut sei = SHELLEXECUTEINFOW {
        cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC | SEE_MASK_FLAG_NO_UI,
        hwnd: std::ptr::null_mut(),
        lpVerb: verb.as_ptr(),
        lpFile: file_w.as_ptr(),
        lpParameters: params_w.as_ptr(),
        lpDirectory: std::ptr::null(),
        nShow: SW_HIDE as i32,
        hInstApp: std::ptr::null_mut(),
        lpIDList: std::ptr::null_mut(),
        lpClass: std::ptr::null(),
        hkeyClass: std::ptr::null_mut(),
        dwHotKey: 0,
        Anonymous: SHELLEXECUTEINFOW_0 {
            hMonitor: std::ptr::null_mut(),
        },
        hProcess: std::ptr::null_mut(),
    };

    if unsafe { ShellExecuteExW(&mut sei) } == 0 {
        let err = unsafe { GetLastError() };
        if err == ERROR_CANCELLED {
            return Err(Error::Process(
                "已取消管理员授权，mihomo 未启动".to_string(),
            ));
        }
        return Err(Error::Process(format!("请求管理员权限失败 (错误码 {err})")));
    }
    Ok(ProcessHandle(sei.hProcess))
}

/// 从 ShellExecuteEx 返回的句柄解析 PID
#[cfg(windows)]
fn process_id_of(handle: ProcessHandle) -> Option<u32> {
    let mut dup: HANDLE = std::ptr::null_mut();
    let ok = unsafe {
        DuplicateHandle(
            GetCurrentProcess(),
            handle.0,
            GetCurrentProcess(),
            &mut dup,
            PROCESS_QUERY_LIMITED_INFORMATION,
            0,
            0,
        )
    };
    if ok == 0 {
        return None;
    }
    let pid = unsafe { GetProcessId(dup) };
    unsafe { CloseHandle(dup) };
    if pid == 0 { None } else { Some(pid) }
}

#[cfg(windows)]
fn spawn_mihomo_elevated(binary: &ResolvedBinary, config_dir: &Path) -> Result<u32, Error> {
    let params = format!("-d \"{}\"", config_dir.to_string_lossy());
    let handle = shell_run_elevated(&binary.cmd, &params)?;
    let pid = process_id_of(handle).ok_or(Error::Process("无法获取提权进程 PID".to_string()))?;
    if let Ok(mut slot) = ELEVATED_PROCESS.get_or_init(|| Mutex::new(None)).lock() {
        *slot = Some((pid, handle));
    }
    Ok(pid)
}

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
        let pid = spawn_mihomo_elevated(&binary, config_dir)?;
        save_pid_elevated(config_dir, pid)?;
        return Ok((pid, binary));
    }

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

    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x00000200;
        const DETACHED_PROCESS: u32 = 0x00000008;
        cmd.creation_flags(CREATE_NEW_PROCESS_GROUP | DETACHED_PROCESS);
    }

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        unsafe {
            cmd.pre_exec(|| {
                if libc::setsid() < 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
    }

    let child = cmd
        .spawn()
        .map_err(|e| Error::Process(format!("启动 mihomo 失败: {e}")))?;
    let pid = child.id();
    save_pid(config_dir, pid)?;
    Ok((pid, binary))
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

#[cfg(windows)]
fn cached_elevated_handle(pid: u32) -> Option<ProcessHandle> {
    ELEVATED_PROCESS
        .get()?
        .lock()
        .ok()?
        .and_then(|slot| (slot.0 == pid).then_some(slot.1))
}

/// 提权执行 taskkill（句柄缓存失效时兜底，会再弹一次 UAC）
#[cfg(windows)]
fn kill_pid_via_runas(pid: u32) -> Result<(), Error> {
    shell_run_elevated("taskkill", &format!("/F /T /PID {pid}"))?;
    Ok(())
}

#[cfg_attr(not(windows), allow(unused_variables))]
fn kill_pid(pid: u32, elevated: bool) -> Result<(), Error> {
    #[cfg(windows)]
    {
        if elevated {
            if let Some(handle) = cached_elevated_handle(pid) {
                if unsafe { TerminateProcess(handle.0, 1) } != 0 {
                    if let Some(cell) = ELEVATED_PROCESS.get() {
                        if let Ok(mut slot) = cell.lock() {
                            *slot = None;
                        }
                    }
                    return Ok(());
                }
            }
            return kill_pid_via_runas(pid);
        }
        let output = Command::new("taskkill")
            .args(["/F", "/T", "/PID", &pid.to_string()])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .output()
            .map_err(|e| Error::Process(format!("执行 taskkill 失败: {e}")))?;
        if output.status.success() {
            Ok(())
        } else {
            Err(Error::Process(format!(
                "停止进程(PID {pid})失败: {}",
                String::from_utf8_lossy(&output.stderr)
            )))
        }
    }
    #[cfg(unix)]
    {
        unsafe {
            libc::kill(pid as i32, libc::SIGTERM);
        }
        let deadline = std::time::Instant::now() + Duration::from_secs(3);
        while is_pid_alive(pid) && std::time::Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(100));
        }
        if is_pid_alive(pid) {
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
}

// ===== 查找 mihomo PID（TUN 权限检查用）=====

#[cfg(unix)]
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

#[cfg(unix)]
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

#[cfg(windows)]
fn mihomo_image_name(mihomo_path: &str) -> String {
    std::path::Path::new(mihomo_path)
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| mihomo_path.to_string())
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use crate::core::mihomo::binary::{BinarySource, ResolvedBinary};

    fn binary(cmd: &str) -> ResolvedBinary {
        ResolvedBinary {
            cmd: cmd.to_string(),
            source: BinarySource::Path,
        }
    }

    #[test]
    fn cmdline_match_wins_over_image_name() {
        // 回归：旧版/裸名解析出 "mihomo.exe"，但实际进程是 winget 包名，
        // 只要命令行包含 config_dir 就应判定为本程序启动
        let config_dir = Path::new(r"C:\Users\rimyn\AppData\Roaming\coclash");
        assert!(matches_mihomo_info(
            "mihomo-windows-amd64.exe",
            r#""C:\...\mihomo-windows-amd64.exe" -d C:\Users\rimyn\AppData\Roaming\coclash"#,
            &binary("mihomo.exe"),
            config_dir,
        ));
    }

    #[test]
    fn image_name_fallback() {
        let config_dir = Path::new(r"C:\Users\rimyn\AppData\Roaming\coclash");
        // 命令行不含 config_dir 且镜像名不匹配 → false
        assert!(!matches_mihomo_info(
            "other.exe",
            r"C:\other\other.exe -d C:\somewhere\else",
            &binary("mihomo.exe"),
            config_dir,
        ));
        // 镜像名匹配（忽略大小写）→ true
        assert!(matches_mihomo_info(
            "MIHOMO.EXE",
            "",
            &binary(r"C:\opt\mihomo.exe"),
            config_dir,
        ));
    }

    #[test]
    fn empty_config_dir_does_not_match_everything() {
        // 空 config_dir 时不能仅凭 contains("") 命中 cmdline 检查
        let config_dir = Path::new("");
        assert!(!matches_mihomo_info(
            "other.exe",
            "",
            &binary("mihomo.exe"),
            config_dir,
        ));
    }
}
