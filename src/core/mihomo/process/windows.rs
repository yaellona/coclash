//! Windows 平台实现：UAC 提权启动（ShellExecuteExW）、taskkill/TerminateProcess 停止、
//! PowerShell CIM / tasklist 进程信息探测。
use super::super::binary::ResolvedBinary;
use super::pidfile_path;
use crate::error::Error;
use std::fs;
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

fn spawn_mihomo_elevated(binary: &ResolvedBinary, config_dir: &Path) -> Result<u32, Error> {
    let params = format!("-d \"{}\"", config_dir.to_string_lossy());
    let handle = shell_run_elevated(&binary.cmd, &params)?;
    let pid = process_id_of(handle).ok_or(Error::Process("无法获取提权进程 PID".to_string()))?;
    if let Ok(mut slot) = ELEVATED_PROCESS.get_or_init(|| Mutex::new(None)).lock() {
        *slot = Some((pid, handle));
    }
    Ok(pid)
}

/// 提权启动 + 写入 `{pid}:1` 标记的 pidfile（供 `stop_mihomo` 识别）
pub(super) fn start_elevated(binary: &ResolvedBinary, config_dir: &Path) -> Result<u32, Error> {
    let pid = spawn_mihomo_elevated(binary, config_dir)?;
    fs::write(pidfile_path(config_dir), format!("{pid}:1"))
        .map_err(|e| Error::Process(format!("写入 PID 文件失败: {e}")))?;
    Ok(pid)
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

/// 判定依据：命令行包含 config_dir 为主（与 unix 语义一致，不依赖解析到的 mihomo 文件名，
/// 避免不同解析结果互相误判为外部并清掉 pidfile）；镜像名比对为辅助兜底。
pub(super) fn is_mihomo_pid(binary: &ResolvedBinary, config_dir: &Path, pid: u32) -> bool {
    if let Some((name, cmdline)) = process_info(pid) {
        matches_mihomo_info(&name, &cmdline, binary, config_dir)
    } else {
        // CIM 查询失败时退回镜像名比对
        image_name(pid)
            .map(|img| img.eq_ignore_ascii_case(&mihomo_image_name(&binary.cmd)))
            .unwrap_or(false)
    }
}

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
fn process_info(pid: u32) -> Option<(String, String)> {
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

fn mihomo_image_name(mihomo_path: &str) -> String {
    std::path::Path::new(mihomo_path)
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| mihomo_path.to_string())
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

pub(super) fn kill_pid(pid: u32, elevated: bool) -> Result<(), Error> {
    if elevated {
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

#[cfg(test)]
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
