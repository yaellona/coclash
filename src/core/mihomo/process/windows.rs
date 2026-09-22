//! Windows 平台实现：自删除临时文件启动（`%TEMP%`，进程退出即删）、
//! UAC 提权启动（ShellExecuteExW）、taskkill/TerminateProcess 停止、
//! netstat / tasklist 进程信息探测。
//!
//! Windows 无法从内存执行 PE，内核默认解压为 `%TEMP%\coclash-<pid>\mihomo.exe`：
//! 启动后由等待线程在「内核进程退出且 coclash 仍在运行」时删除整个目录。
//! TUI 与内核解耦（关 TUI 不关内核），coclash 先退出时目录会暂时保留，
//! 由下次启动的 `cleanup_stale_temp` 清理；临时文件创建失败时回退缓存目录。
use crate::constants::MIHOMO_LOG_FILE;
use crate::error::Error;
use crate::settings::Settings;
use std::ffi::OsString;
use std::fs::{self, OpenOptions};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Mutex, OnceLock};
use windows_sys::Win32::Foundation::{
    CloseHandle, DuplicateHandle, ERROR_CANCELLED, GetLastError, HANDLE,
};
use windows_sys::Win32::System::Threading::{
    GetCurrentProcess, GetProcessId, INFINITE, PROCESS_QUERY_LIMITED_INFORMATION, TerminateProcess,
    WaitForSingleObject,
};
use windows_sys::Win32::UI::Shell::{
    SEE_MASK_FLAG_NO_UI, SEE_MASK_NOASYNC, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW,
    SHELLEXECUTEINFOW_0, ShellExecuteExW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::SW_HIDE;

use super::BinarySource;
use super::embedded;

/// 进程句柄在进程间共享是安全的（句柄值本身可被多个线程使用）
#[derive(Clone, Copy)]
struct ProcessHandle(HANDLE);
unsafe impl Send for ProcessHandle {}
unsafe impl Sync for ProcessHandle {}

/// 提权启动的 mihomo 进程句柄缓存（本进程生命周期内有效）
static ELEVATED_PROCESS: OnceLock<Mutex<Option<(u32, ProcessHandle)>>> = OnceLock::new();

// ===== 内核来源物化 =====

/// 物化后的内核可执行来源（Windows：自删除临时文件或缓存文件）
pub(super) enum PreparedExec {
    /// `%TEMP%\coclash-<pid>\mihomo.exe`：进程退出后由等待线程删除整个目录
    Temp { path: PathBuf, dir: PathBuf },
    /// 缓存目录（临时文件创建失败/显式强制落盘）
    Cache(PathBuf),
}

impl PreparedExec {
    fn path(&self) -> &Path {
        match self {
            Self::Temp { path, .. } => path,
            Self::Cache(path) => path,
        }
    }

    fn source(&self) -> BinarySource {
        match self {
            Self::Temp { path, .. } => BinarySource::TempFile(path.clone()),
            Self::Cache(path) => BinarySource::CacheFile(path.clone()),
        }
    }

    /// 进程退出后需要清理的临时目录
    fn cleanup_dir(&self) -> Option<&Path> {
        match self {
            Self::Temp { dir, .. } => Some(dir),
            Self::Cache(_) => None,
        }
    }
}

/// 物化内核：解压为临时文件，失败时回退缓存目录
pub(super) fn prepare_exec() -> Result<PreparedExec, Error> {
    match prepare_temp() {
        Ok(prepared) => Ok(prepared),
        Err(e) => {
            eprintln!("coclash: 临时文件释放失败({e})，回退解压到缓存目录");
            Ok(PreparedExec::Cache(embedded::ensure_extracted()?))
        }
    }
}

fn prepare_temp() -> Result<PreparedExec, Error> {
    let raw = embedded::decompress_embedded()?;
    let dir = std::env::temp_dir().join(format!("coclash-{}", std::process::id()));
    cleanup_stale_temp(&dir);
    fs::create_dir_all(&dir)?;
    let path = dir.join("mihomo.exe");
    fs::write(&path, &raw)?;
    Ok(PreparedExec::Temp { path, dir })
}

/// 清理历史遗留的临时内核目录：`%TEMP%\coclash-<pid>` 中进程已退出的目录
/// （coclash 先于内核退出时，等待线程已消失，只能留到下次启动清理）
fn cleanup_stale_temp(keep: &Path) {
    let Ok(entries) = fs::read_dir(std::env::temp_dir()) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let Some(pid) = name
            .to_string_lossy()
            .strip_prefix("coclash-")
            .and_then(|s| s.parse::<u32>().ok())
        else {
            continue;
        };
        if entry.path() == keep || is_pid_alive(pid) {
            continue;
        }
        let _ = fs::remove_dir_all(entry.path());
    }
}

// ===== 启动 / 替换 =====

/// 启动内核（`elevate` 为 true 时走 UAC 提权），并安排临时文件清理
pub(super) fn start(
    prepared: PreparedExec,
    config_dir: &Path,
    elevate: bool,
) -> Result<(u32, BinarySource), Error> {
    let source = prepared.source();
    if elevate {
        let handle = spawn_mihomo_elevated(prepared.path(), config_dir)?;
        let pid =
            process_id_of(handle).ok_or(Error::Process("无法获取提权进程 PID".to_string()))?;
        if let Ok(mut slot) = ELEVATED_PROCESS.get_or_init(|| Mutex::new(None)).lock() {
            *slot = Some((pid, handle));
        }
        if let Some(dir) = prepared.cleanup_dir() {
            spawn_cleanup(handle, dir.to_path_buf());
        }
        Ok((pid, source))
    } else {
        let pid = spawn_detached(&prepared, config_dir)?;
        Ok((pid, source))
    }
}

/// `coclash core`：等待 mihomo 结束并透传退出码（Windows 无法 exec 替换进程）
pub(super) fn exec_replace(prepared: &PreparedExec, args: &[OsString]) -> Result<(), Error> {
    let mut cmd = Command::new(prepared.path());
    cmd.args(args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    let status = cmd
        .status()
        .map_err(|e| Error::Process(format!("启动 mihomo 失败: {e}")))?;
    if let Some(dir) = prepared.cleanup_dir() {
        let _ = fs::remove_dir_all(dir);
    }
    std::process::exit(status.code().unwrap_or(1));
}

/// 普通（非提权）启动：日志重定向 + 分离会话配置 + 退出后清理临时目录
fn spawn_detached(prepared: &PreparedExec, config_dir: &Path) -> Result<u32, Error> {
    let mut cmd = Command::new(prepared.path());
    cmd.arg("-d").arg(config_dir).stdin(Stdio::null());

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

    configure_detached(&mut cmd)?;
    let mut child = cmd
        .spawn()
        .map_err(|e| Error::Process(format!("启动 mihomo 失败: {e}")))?;
    let pid = child.id();
    if let Some(dir) = prepared.cleanup_dir() {
        let dir = dir.to_path_buf();
        std::thread::spawn(move || {
            let _ = child.wait();
            let _ = fs::remove_dir_all(&dir);
        });
    }
    Ok(pid)
}

/// 等提权 mihomo 退出后删除临时目录（句柄缓存不受影响，仍可正常停止进程）
fn spawn_cleanup(handle: ProcessHandle, dir: PathBuf) {
    std::thread::spawn(move || {
        // 必须把整个 `ProcessHandle` 传进函数：闭包若直接读 `handle.0`
        // 会按字段捕获裸指针，绕过下面的 `unsafe impl Send`
        wait_and_cleanup(handle, dir);
    });
}

fn wait_and_cleanup(handle: ProcessHandle, dir: PathBuf) {
    unsafe {
        WaitForSingleObject(handle.0, INFINITE);
    }
    let _ = fs::remove_dir_all(&dir);
}

/// 分离会话：新进程组 + 无控制台（mihomo 持续在后台运行）
fn configure_detached(cmd: &mut Command) -> Result<(), Error> {
    use std::os::windows::process::CommandExt;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x00000200;
    const DETACHED_PROCESS: u32 = 0x00000008;
    cmd.creation_flags(CREATE_NEW_PROCESS_GROUP | DETACHED_PROCESS);
    Ok(())
}

// ===== UAC 提权 =====

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

fn spawn_mihomo_elevated(binary: &Path, config_dir: &Path) -> Result<ProcessHandle, Error> {
    let params = format!("-d \"{}\"", config_dir.to_string_lossy());
    shell_run_elevated(&binary.to_string_lossy(), &params)
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

    #[test]
    fn temp_prepared_source_and_cleanup() {
        let prepared = PreparedExec::Temp {
            path: PathBuf::from(r"C:\Temp\coclash-1\mihomo.exe"),
            dir: PathBuf::from(r"C:\Temp\coclash-1"),
        };
        assert_eq!(prepared.path(), Path::new(r"C:\Temp\coclash-1\mihomo.exe"));
        assert!(prepared.cleanup_dir().is_some());
        assert!(prepared.source().to_string().contains("临时文件"));

        let cache = PreparedExec::Cache(PathBuf::from(r"C:\Cache\mihomo.exe"));
        assert!(cache.cleanup_dir().is_none());
        assert!(cache.source().to_string().contains("磁盘文件"));
    }
}
