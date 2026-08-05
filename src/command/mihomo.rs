use crate::config::node::ProxyReport;
use crate::constants::{MIHOMO_LOG_FILE, PID_FILE, SUBSCRIPTION_UA};
use crate::settings::Settings;
use reqwest;
use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

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

// ===== mihomo 二进制解析 =====

pub const ENV_VAR_NAME: &str = "TECLASH_MIHOMO_EXE";
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

/// 解析链：环境变量 > 同目录 .env > settings 显式路径 > NixOS wrapper > PATH
pub fn resolve_mihomo_exe(settings: &Settings) -> ResolvedBinary {
    let env_val = std::env::var(ENV_VAR_NAME).ok();
    let env_file = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.join(".env")));
    resolve_mihomo_exe_with(settings, env_val, env_file.as_deref())
}

pub(crate) fn resolve_mihomo_exe_with(
    settings: &Settings,
    env_val: Option<String>,
    env_file: Option<&Path>,
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
    if let Some(path) = env_file {
        if let Ok(content) = fs::read_to_string(path) {
            if let Some(v) = parse_env_file(&content).get(ENV_FILE_KEY) {
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

    // 4. NixOS setcap wrapper（独立于 shell PATH，任何启动方式都能命中）
    #[cfg(unix)]
    if Path::new("/run/wrappers/bin/mihomo").exists() {
        return ResolvedBinary {
            cmd: "/run/wrappers/bin/mihomo".to_string(),
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

// ===== pidfile =====

fn pidfile_path(config_dir: &Path) -> PathBuf {
    config_dir.join(PID_FILE)
}

pub(crate) fn save_pid(config_dir: &Path, pid: u32) -> Result<(), String> {
    fs::write(pidfile_path(config_dir), pid.to_string())
        .map_err(|e| format!("写入 PID 文件失败: {e}"))
}

/// 记录提权启动的进程，pidfile 写入 `{pid}:1` 标记
pub(crate) fn save_pid_elevated(config_dir: &Path, pid: u32) -> Result<(), String> {
    fs::write(pidfile_path(config_dir), format!("{pid}:1"))
        .map_err(|e| format!("写入 PID 文件失败: {e}"))
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

// ===== 进程探测 =====

pub fn is_port_up(settings: &Settings) -> bool {
    let addr: std::net::SocketAddr = match settings.mihomo_ctrl_addr.parse() {
        Ok(a) => a,
        Err(_) => return false,
    };
    TcpStream::connect_timeout(&addr, Duration::from_millis(300)).is_ok()
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
#[cfg_attr(target_os = "windows", allow(unused_variables))]
fn is_mihomo_pid(binary: &ResolvedBinary, config_dir: &Path, pid: u32) -> bool {
    if !is_pid_alive(pid) {
        return false;
    }
    #[cfg(windows)]
    {
        windows_image_name(pid)
            .map(|img| img.eq_ignore_ascii_case(&mihomo_image_name(&binary.cmd)))
            .unwrap_or(false)
    }
    #[cfg(unix)]
    {
        fs::read(format!("/proc/{pid}/cmdline"))
            .map(|c| {
                let cmdline = String::from_utf8_lossy(&c);
                cmdline.contains(config_dir.to_str().unwrap_or(""))
            })
            .unwrap_or(false)
    }
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
fn shell_run_elevated(file: &str, params: &str) -> Result<ProcessHandle, String> {
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
            return Err("已取消管理员授权，mihomo 未启动".to_string());
        }
        return Err(format!("请求管理员权限失败 (错误码 {err})"));
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
fn spawn_mihomo_elevated(binary: &ResolvedBinary, config_dir: &Path) -> Result<u32, String> {
    let params = format!("-d \"{}\"", config_dir.to_string_lossy());
    let handle = shell_run_elevated(&binary.cmd, &params)?;
    let pid = process_id_of(handle).ok_or("无法获取提权进程 PID")?;
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
) -> Result<(u32, ResolvedBinary), String> {
    let config_dir = config_path.parent().ok_or("无法获取配置目录")?;
    if is_port_up(settings) {
        return Err("端口已被 mihomo 占用，未启动新进程".to_string());
    }

    let binary = resolve_mihomo_exe(settings);

    #[cfg(windows)]
    if elevate {
        let pid = spawn_mihomo_elevated(&binary, config_dir)?;
        save_pid_elevated(config_dir, pid)?;
        return Ok((pid, binary));
    }

    let mut cmd = Command::new(&binary.cmd);
    cmd.args(["-d", config_dir.to_str().ok_or("config路径无效")?])
        .stdin(Stdio::null());

    // mihomo 的 stdout/stderr 重定向到日志文件，便于排查启动失败
    let log_path = config_dir.join(MIHOMO_LOG_FILE);
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .map_err(|e| format!("打开日志文件失败: {e}"))?;
    let file2 = file
        .try_clone()
        .map_err(|e| format!("克隆日志文件失败: {e}"))?;
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

    let child = cmd.spawn().map_err(|e| format!("启动 mihomo 失败: {e}"))?;
    let pid = child.id();
    save_pid(config_dir, pid)?;
    Ok((pid, binary))
}

/// 只停止由本程序启动的 mihomo（依据 pidfile）；外部实例拒绝操作
pub fn stop_mihomo(settings: &Settings, config_dir: &Path) -> Result<(), String> {
    let (pid, elevated) = match load_pidfile_elevated(config_dir) {
        Some(p) => p,
        None => {
            return Err(
                "未找到由本程序启动的 mihomo（无 PID 记录）；若是外部启动的实例，请自行关闭"
                    .to_string(),
            );
        }
    };
    let binary = resolve_mihomo_exe(settings);
    if !is_mihomo_pid(&binary, config_dir, pid) {
        clear_pidfile(config_dir);
        return Err(format!(
            "PID 记录 ({pid}) 已失效（进程不存在或不是 mihomo），已清除记录"
        ));
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
fn kill_pid_via_runas(pid: u32) -> Result<(), String> {
    shell_run_elevated("taskkill", &format!("/F /T /PID {pid}"))?;
    Ok(())
}

#[cfg_attr(not(windows), allow(unused_variables))]
fn kill_pid(pid: u32, elevated: bool) -> Result<(), String> {
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
            .map_err(|e| format!("执行 taskkill 失败: {e}"))?;
        if output.status.success() {
            Ok(())
        } else {
            Err(format!(
                "停止进程(PID {pid})失败: {}",
                String::from_utf8_lossy(&output.stderr)
            ))
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

// ===== 查找 mihomo PID =====

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
        if let Ok(cmdline) = fs::read(base.join("cmdline")) {
            if String::from_utf8_lossy(&cmdline).contains(config_dir) {
                return Some(pid);
            }
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

fn mihomo_image_name(mihomo_path: &str) -> String {
    std::path::Path::new(mihomo_path)
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| mihomo_path.to_string())
}

// ===== 异步 API 调用 =====

pub async fn fetch_delays(settings: &Settings) -> Result<HashMap<String, u32>, String> {
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(settings.delay_http_timeout())
        .build()
        .map_err(|e| format!("创建HTTP客户端失败: {e}"))?;
    let url = format!(
        "{}/group/Proxy/delay?timeout={}&url={}",
        settings.mihomo_api, settings.delay_timeout_ms, settings.test_url
    );
    let body = client
        .get(url)
        .send()
        .await
        .map_err(|e| format!("测速请求失败: {e}"))?
        .text()
        .await
        .map_err(|e| format!("读取响应失败: {e}"))?;
    serde_json::from_str::<HashMap<String, u32>>(&body).map_err(|e| format!("解析延迟失败: {e}"))
}

pub async fn reload_config(settings: &Settings, path: PathBuf) -> Result<(), String> {
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(settings.http_timeout())
        .build()
        .map_err(|e| format!("创建HTTP客户端失败: {}", e))?;

    let body = serde_json::json!({ "path": path.to_string_lossy(), "payload": "" });
    let url = format!("{}/configs?force=true", settings.mihomo_api);
    let resp = client
        .put(url)
        .json(&body)
        .send()
        .await
        .map_err(|e| format!("重载配置失败: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!("重载配置失败：API返回状态码 {}", resp.status()));
    }
    Ok(())
}

pub async fn get_proxy(settings: &Settings) -> Result<ProxyReport, String> {
    let client: reqwest::Client = reqwest::Client::builder()
        .no_proxy()
        .build()
        .map_err(|e| e.to_string())?;
    let url = format!("{}/proxies/Proxy", settings.mihomo_api);
    let body = client
        .get(url)
        .send()
        .await
        .map_err(|e| e.to_string())?
        .text()
        .await
        .map_err(|e| e.to_string())?;
    let mihomo_report: ProxyReport =
        serde_json::from_str(&body).map_err(|e| format!("解析节点失败: {e}"))?;
    Ok(mihomo_report)
}

pub async fn switch_node(settings: &Settings, node_name: String) -> Result<(), String> {
    let client = reqwest::Client::builder()
        .no_proxy()
        .build()
        .map_err(|e| e.to_string())?;
    let url = format!("{}/proxies/Proxy", settings.mihomo_api);
    let body = serde_json::json!({ "name": node_name });
    let resp = client
        .put(url)
        .json(&body)
        .send()
        .await
        .map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("切换节点失败：API返回状态码 {}", resp.status()));
    }
    Ok(())
}

// ===== 用flclash的方式来获取代理商的名称 =====

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut result = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(byte) = u8::from_str_radix(
                std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("xx"),
                16,
            ) {
                result.push(byte);
                i += 3;
                continue;
            }
        }
        result.push(bytes[i]);
        i += 1;
    }
    String::from_utf8(result).unwrap_or_default()
}

fn parse_content_disposition(cd: &str) -> Option<String> {
    for part in cd.split(';') {
        let p = part.trim();
        if p.to_lowercase().starts_with("filename*=") {
            let val = &p[10..];
            let segs: Vec<&str> = val.split('\'').collect();
            let encoded = if segs.len() >= 3 { segs[2] } else { val };
            return Some(percent_decode(encoded));
        }
    }
    for part in cd.split(';') {
        let p = part.trim();
        if p.to_lowercase().starts_with("filename=") {
            let val = &p[9..];
            return Some(val.trim_matches('"').trim_matches('\'').to_string());
        }
    }
    None
}

pub async fn get_provider_name(settings: &Settings, url: String) -> Result<String, String> {
    let domain = url::Url::parse(&url)
        .ok()
        .and_then(|u| u.host_str().map(|s| s.to_string()));
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(settings.delay_http_timeout())
        .build()
        .map_err(|e| format!("创建HTTP客户端失败: {e}"))?;
    let resp = client
        .get(&url)
        .header("User-Agent", SUBSCRIPTION_UA)
        .send()
        .await;
    let cd = match &resp {
        Ok(resp) => resp
            .headers()
            .get("content-disposition")
            .and_then(|v| v.to_str().ok())
            .unwrap_or(""),
        Err(e) => {
            if let Some(d) = domain {
                return Ok(d);
            }
            return Err(format!("请求失败: {e}"));
        }
    };
    if let Some(name) = parse_content_disposition(cd) {
        return Ok(name);
    }
    if let Some(d) = domain {
        return Ok(d);
    }
    Err("无法解析订阅名称".to_string())
}
