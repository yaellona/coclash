//! Windows 平台实现：UAC 提权启动（ShellExecuteExW）、taskkill/TerminateProcess 停止、
//! PowerShell CIM / tasklist 进程信息探测。
use crate::error::Error;
use crate::settings::Settings;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::{Mutex, OnceLock};
use windows_sys::Win32::Foundation::{
    CloseHandle, DuplicateHandle, ERROR_CANCELLED, GetLastError, HANDLE,
};
use windows_sys::Win32::System::Threading::{
    GetCurrentProcess, GetProcessId, PROCESS_QUERY_LIMITED_INFORMATION, TerminateProcess,
};
use windows_sys::Win32::UI::Shell::{
    SEE_MASK_FLAG_NO_UI, SEE_MASK_NOASYNC, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW,
    SHELLEXECUTEINFOW_0, ShellExecuteExW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::SW_HIDE;

/// 进程句柄在进程间共享是安全的（句柄值本身可被多个线程使用）
#[derive(Clone, Copy)]
struct ProcessHandle(HANDLE);
unsafe impl Send for ProcessHandle {}
unsafe impl Sync for ProcessHandle {}

/// 提权启动的 mihomo 进程句柄缓存（本进程生命周期内有效）
static ELEVATED_PROCESS: OnceLock<Mutex<Option<(u32, ProcessHandle)>>> = OnceLock::new();

// ===== 启动 =====

fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// 通过 UAC 提示以管理员权限执行程序，返回 hProcess 句柄
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
        nShow: SW_HIDE,
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

fn spawn_mihomo_elevated(binary: &Path, config_dir: &Path) -> Result<u32, Error> {
    let params = format!("-d \"{}\"", config_dir.to_string_lossy());
    let handle = shell_run_elevated(&binary.to_string_lossy(), &params)?;
    let pid = process_id_of(handle).ok_or(Error::Process("无法获取提权进程 PID".to_string()))?;
    if let Ok(mut slot) = ELEVATED_PROCESS.get_or_init(|| Mutex::new(None)).lock() {
        *slot = Some((pid, handle));
    }
    Ok(pid)
}

/// 提权启动；进程句柄缓存在本进程内（供停止时 TerminateProcess）
pub(super) fn start_elevated(binary: &Path, config_dir: &Path) -> Result<u32, Error> {
    spawn_mihomo_elevated(binary, config_dir)
}

/// 分离会话：新进程组 + 无控制台（mihomo 持续在后台运行）
pub(super) fn configure_detached(cmd: &mut Command) -> Result<(), Error> {
    use std::os::windows::process::CommandExt;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x00000200;
    const DETACHED_PROCESS: u32 = 0x00000008;
    cmd.creation_flags(CREATE_NEW_PROCESS_GROUP | DETACHED_PROCESS);
    Ok(())
}

// ===== 进程探测 =====

pub(super) fn is_pid_alive(pid: u32) -> bool {
    image_name(pid).is_some()
}

/// 镜像名是否属于 mihomo（内嵌释放名为 `mihomo.exe`，兼容 winget 的
/// `mihomo-windows-amd64.exe` 等命名）
fn name_matches(name: &str) -> bool {
    name.to_ascii_lowercase().starts_with("mihomo")
}

/// 找到监听控制端口的 mihomo 进程（不区分是否由本程序启动）
pub(super) fn find_mihomo_pid(settings: &Settings) -> Option<u32> {
    let port = super::ctrl_port(&settings.mihomo_ctrl_addr)?;
    let output = Command::new("netstat")
        .args(["-ano", "-p", "tcp"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let pid = parse_netstat_listen_pid(&String::from_utf8_lossy(&output.stdout), port)?;
    // 只接受 mihomo 进程，避免误杀占用同一端口的其它程序
    image_name(pid)
        .is_some_and(|n| name_matches(&n))
        .then_some(pid)
}

/// 解析 `netstat -ano -p tcp`：返回本地端口为 `port` 的 LISTENING 行 PID
fn parse_netstat_listen_pid(output: &str, port: u16) -> Option<u32> {
    let suffix = format!(":{port}");
    for line in output.lines() {
        let cols: Vec<&str> = line.split_whitespace().collect();
        // 形如: TCP  127.0.0.1:19090  0.0.0.0:0  LISTENING  12345
        if cols.len() < 5 || !cols[0].eq_ignore_ascii_case("TCP") || cols[3] != "LISTENING" {
            continue;
        }
        if cols[1].ends_with(&suffix) {
            return cols[4].parse().ok();
        }
    }
    None
}

fn image_name(pid: u32) -> Option<String> {
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

// ===== 停止 =====

fn cached_elevated_handle(pid: u32) -> Option<ProcessHandle> {
    ELEVATED_PROCESS
        .get()?
        .lock()
        .ok()?
        .and_then(|slot| (slot.0 == pid).then_some(slot.1))
}

/// 提权执行 taskkill（句柄缓存失效时兜底，会再弹一次 UAC）
fn kill_pid_via_runas(pid: u32) -> Result<(), Error> {
    shell_run_elevated("taskkill", &format!("/F /T /PID {pid}"))?;
    Ok(())
}

/// 停止进程。无文件记录时无法预知是否提权，采用回退链：
/// 本进程缓存的提权句柄 → 普通 taskkill → 提权 taskkill（UAC）。
pub(super) fn kill_pid(pid: u32) -> Result<(), Error> {
    if let Some(handle) = cached_elevated_handle(pid)
        && unsafe { TerminateProcess(handle.0, 1) } != 0
    {
        if let Some(cell) = ELEVATED_PROCESS.get()
            && let Ok(mut slot) = cell.lock()
        {
            *slot = None;
        }
        return Ok(());
    }
    match taskkill(pid) {
        Ok(()) => Ok(()),
        // 提权进程普通 taskkill 会拒绝（access denied），退回 UAC 提权执行
        Err(_) => kill_pid_via_runas(pid),
    }
}

fn taskkill(pid: u32) -> Result<(), Error> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_matches_mihomo_variants() {
        assert!(name_matches("mihomo.exe"));
        assert!(name_matches("MIHOMO-WINDOWS-AMD64.EXE"));
        assert!(!name_matches("other.exe"));
        assert!(!name_matches(""));
    }

    #[test]
    fn parse_netstat_listening_pid() {
        let output = "\
  TCP    127.0.0.1:9090         0.0.0.0:0              LISTENING       3584
  TCP    127.0.0.1:19090        0.0.0.0:0              LISTENING       12868
  TCP    [::]:19090             [::]:0                 LISTENING       12868
  TCP    127.0.0.1:7890         127.0.0.1:52344        ESTABLISHED     1
";
        assert_eq!(parse_netstat_listen_pid(output, 9090), Some(3584));
        assert_eq!(parse_netstat_listen_pid(output, 19090), Some(12868));
        assert_eq!(parse_netstat_listen_pid(output, 7890), None);
        assert_eq!(parse_netstat_listen_pid("", 9090), None);
    }
}
