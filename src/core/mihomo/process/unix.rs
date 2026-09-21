//! Unix 平台实现：内存执行（Linux memfd + execveat）、setsid 分离会话、
//! SIGTERM→SIGKILL 信号终止、/proc 进程与端口探测。
//!
//! - 内核来源：Linux 默认 memfd（运行期不落盘）；内核不支持、显式要求
//!   （`COCLASH_MIHOMO_DIR` / `COCLASH_MIHOMO_EXTRACT`）或启动失败时回退缓存目录。
//!   其他 Unix 无 memfd，统一走缓存目录。
//! - 权限传递：父进程若带 CAP_NET_ADMIN/CAP_NET_RAW（对 coclash 本体 setcap），
//!   启动前以 ambient 方式传给 mihomo，TUN 无需给释放文件单独 setcap。
//! - 进程识别：兼容磁盘可执行与内存执行（`/proc/<pid>/exe` 为
//!   `/memfd:mihomo (deleted)`，内核 ≥6.10 时 `comm` 为 `memfd:mihomo`）。
use crate::constants::MIHOMO_LOG_FILE;
use crate::error::Error;
use crate::settings::Settings;
use std::ffi::{CString, OsString};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use super::BinarySource;
use super::embedded;

// ===== 内核来源物化 =====

/// 物化后的内核可执行来源（Unix：内存 fd 或磁盘路径）
pub(super) enum PreparedExec {
    /// Linux memfd：内核字节已写入匿名内存文件
    MemFd(OwnedFd),
    /// 磁盘上的 mihomo（兜底/强制落盘/其他 Unix）
    Cache(PathBuf),
}

impl PreparedExec {
    fn source(&self) -> BinarySource {
        match self {
            Self::MemFd(_) => BinarySource::Memory,
            Self::Cache(path) => BinarySource::CacheFile(path.clone()),
        }
    }
}

/// 物化内核：默认内存（Linux memfd），失败或显式要求时解压到缓存目录
pub(super) fn prepare_exec() -> Result<PreparedExec, Error> {
    if force_extract() {
        return Ok(PreparedExec::Cache(embedded::ensure_extracted()?));
    }
    match memfd_exec() {
        Ok(fd) => Ok(PreparedExec::MemFd(fd)),
        Err(e) => {
            eprintln!("coclash: 内存执行不可用({e})，回退解压到缓存目录");
            Ok(PreparedExec::Cache(embedded::ensure_extracted()?))
        }
    }
}

/// `COCLASH_MIHOMO_DIR` 指定缓存根即强制落盘；`COCLASH_MIHOMO_EXTRACT` 非 0 同理
fn force_extract() -> bool {
    std::env::var_os("COCLASH_MIHOMO_DIR").is_some()
        || std::env::var_os("COCLASH_MIHOMO_EXTRACT").is_some_and(|v| v != "0")
}

/// Linux：memfd_create + 写入解压后的内核字节（fd 带 CLOEXEC）
#[cfg(target_os = "linux")]
fn memfd_exec() -> Result<OwnedFd, Error> {
    use std::fs::File;

    let raw = embedded::decompress_embedded()?;
    let fd = unsafe { libc::memfd_create(c"mihomo".as_ptr(), libc::MFD_CLOEXEC) };
    if fd < 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    let mut file = unsafe { File::from_raw_fd(fd) };
    file.write_all(&raw)?;
    Ok(file.into())
}

/// 非 Linux：无 memfd，交给调用方回退缓存目录
#[cfg(not(target_os = "linux"))]
fn memfd_exec() -> Result<OwnedFd, Error> {
    Err(Error::Process("当前平台不支持 memfd 内存执行".to_string()))
}

// ===== 启动 / 替换 =====

/// 启动内核：内存启动失败时自动回退缓存目录，返回实际来源
pub(super) fn start(
    prepared: PreparedExec,
    config_dir: &Path,
    _elevate: bool,
) -> Result<(u32, BinarySource), Error> {
    match spawn(&prepared, config_dir) {
        Ok(pid) => Ok((pid, prepared.source())),
        Err(e) if matches!(prepared, PreparedExec::MemFd(_)) => {
            eprintln!("coclash: 内存启动失败({e})，回退解压到缓存目录");
            let path = embedded::ensure_extracted()?;
            let pid = spawn(&PreparedExec::Cache(path.clone()), config_dir)?;
            Ok((pid, BinarySource::CacheFile(path)))
        }
        Err(e) => Err(e),
    }
}

/// `coclash core`：替换当前进程为 mihomo（PID 不变，信号/终端/退出码原生）
pub(super) fn exec_replace(prepared: &PreparedExec, args: &[OsString]) -> Result<(), Error> {
    match prepared {
        PreparedExec::MemFd(fd) => {
            raise_ambient_caps();
            let exec = ExecArgs::new(args.iter().cloned())?;
            let rc = unsafe {
                libc::execveat(
                    fd.as_raw_fd(),
                    c"".as_ptr(),
                    exec.argv_ptr(),
                    exec.envp_ptr(),
                    libc::AT_EMPTY_PATH,
                )
            };
            if rc < 0 {
                let e = std::io::Error::last_os_error();
                eprintln!("coclash: 内存执行失败({e})，回退解压");
                return exec_path(&embedded::ensure_extracted()?, args);
            }
            unreachable!("execveat 失败时必然走错误分支")
        }
        PreparedExec::Cache(path) => exec_path(path, args),
    }
}

/// 以磁盘文件替换进程
fn exec_path(path: &Path, args: &[OsString]) -> Result<(), Error> {
    raise_ambient_caps();
    let mut cmd = Command::new(path);
    cmd.args(args);
    Err(cmd.exec().into())
}

/// 非提权启动：stdin 置空、stdout/stderr 追加到 mihomo.log，平台维度在 `command` 中完成
fn spawn(prepared: &PreparedExec, config_dir: &Path) -> Result<u32, Error> {
    let mut cmd = command(prepared, config_dir)?;
    cmd.stdin(Stdio::null());

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

    let child = cmd
        .spawn()
        .map_err(|e| Error::Process(format!("启动 mihomo 失败: {e}")))?;
    Ok(child.id())
}

/// 构造待 spawn 的 Command：
/// - 内存执行：程序路径为 `/proc/self/fd/<fd>`，真正的 exec 在 pre_exec 中用
///   `execveat(AT_EMPTY_PATH)` 完成（保留 memfd 进程名/来源描述）；
/// - 磁盘路径：直接执行。
///
/// 两种情况都在 pre_exec 中 setsid 分离会话并传递 ambient capabilities。
fn command(prepared: &PreparedExec, config_dir: &Path) -> Result<Command, Error> {
    match prepared {
        PreparedExec::MemFd(fd) => {
            let exec = ExecArgs::new(["-d".into(), config_dir.as_os_str().to_os_string()])?;
            let raw_fd = fd.as_raw_fd();
            let mut cmd = Command::new(format!("/proc/self/fd/{raw_fd}"));
            // SAFETY: pre_exec 仅调用 setsid/prctl/execveat 并读取预构造指针表，不分配内存
            unsafe {
                cmd.pre_exec(move || {
                    detach_session()?;
                    raise_ambient_caps();
                    exec_fd(raw_fd, &exec)
                });
            }
            Ok(cmd)
        }
        PreparedExec::Cache(path) => {
            let mut cmd = Command::new(path);
            cmd.arg("-d").arg(config_dir);
            // SAFETY: pre_exec 仅调用 setsid/prctl，不分配内存
            unsafe {
                cmd.pre_exec(|| {
                    detach_session()?;
                    raise_ambient_caps();
                    Ok(())
                });
            }
            Ok(cmd)
        }
    }
}

/// pre_exec 内执行 execveat；成功即进程已被替换（不返回），失败返回 errno
#[cfg(target_os = "linux")]
fn exec_fd(fd: RawFd, exec: &ExecArgs) -> std::io::Result<()> {
    let rc = unsafe {
        libc::execveat(
            fd,
            c"".as_ptr(),
            exec.argv_ptr(),
            exec.envp_ptr(),
            libc::AT_EMPTY_PATH,
        )
    };
    if rc < 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

/// 非 Linux 不会构造 `MemFd`（`memfd_exec` 直接报错），此实现仅保证可编译
#[cfg(not(target_os = "linux"))]
fn exec_fd(_fd: RawFd, _exec: &ExecArgs) -> std::io::Result<()> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "当前平台不支持 execveat",
    ))
}

/// 分离会话（setsid），使 mihomo 在终端退出后继续运行
fn detach_session() -> std::io::Result<()> {
    if unsafe { libc::setsid() } < 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

// ===== execve 参数（fork 前预构造，pre_exec 中零分配）=====

/// argv/envp 的 CString 与对应指针表；指针指向自身拥有的 CString 堆内存，
/// 结构体移动不影响有效性。
struct ExecArgs {
    _argv: Vec<CString>,
    _envp: Vec<CString>,
    argv: Vec<*mut libc::c_char>,
    envp: Vec<*mut libc::c_char>,
}

// SAFETY: 指针表只读且指向自身字段拥有的内存，跨线程移动后仍有效
unsafe impl Send for ExecArgs {}
unsafe impl Sync for ExecArgs {}

impl ExecArgs {
    /// `argv[0]` 固定为 `mihomo`，`extra` 原样追加；envp 继承当前进程环境
    fn new(extra: impl IntoIterator<Item = OsString>) -> Result<Self, Error> {
        let argv = std::iter::once(OsString::from("mihomo"))
            .chain(extra)
            .map(|s| CString::new(s.as_os_str().as_bytes()))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| Error::Process("mihomo 参数含 NUL 字符".to_string()))?;
        let envp = std::env::vars_os()
            .filter_map(|(key, value)| {
                let mut joined = key.into_vec();
                joined.push(b'=');
                joined.extend(value.into_vec());
                CString::new(joined).ok()
            })
            .collect::<Vec<_>>();

        let mut args = Self {
            argv: argv
                .iter()
                .map(|c| c.as_ptr() as *mut libc::c_char)
                .collect(),
            envp: envp
                .iter()
                .map(|c| c.as_ptr() as *mut libc::c_char)
                .collect(),
            _argv: argv,
            _envp: envp,
        };
        // execveat 要求指针表以 NULL 结尾
        args.argv.push(std::ptr::null_mut());
        args.envp.push(std::ptr::null_mut());
        Ok(args)
    }

    fn argv_ptr(&self) -> *const *mut libc::c_char {
        self.argv.as_ptr()
    }

    fn envp_ptr(&self) -> *const *mut libc::c_char {
        self.envp.as_ptr()
    }
}

// ===== TUN 权限 =====

/// 把 coclash 继承的 CAP_NET_ADMIN/CAP_NET_RAW 以 ambient 方式传给 mihomo。
/// 需要 coclash 本体带这些 file capabilities（`sudo setcap cap_net_admin,cap_net_raw+eip $(which coclash)`）；
/// 未授权时 prctl 失败，静默忽略。
pub(super) fn raise_ambient_caps() {
    #[cfg(target_os = "linux")]
    unsafe {
        // libc 在部分配置下不导出 CAP_* 常量，直接按内核 uapi 定义
        const CAP_NET_ADMIN: libc::c_int = 12;
        const CAP_NET_RAW: libc::c_int = 13;
        for cap in [CAP_NET_ADMIN, CAP_NET_RAW] {
            libc::prctl(
                libc::PR_CAP_AMBIENT,
                libc::PR_CAP_AMBIENT_RAISE as libc::c_ulong,
                cap as libc::c_ulong,
                0,
                0,
            );
        }
    }
}

// ===== 进程探测 =====

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

/// 在 mihomo 进程里找持有该 socket inode 的 PID
fn find_mihomo_pid_by_inode(inode: &str) -> Option<u32> {
    let needle = format!("socket:[{inode}]");
    for entry in fs::read_dir("/proc").ok()?.flatten() {
        let pid: u32 = match entry.file_name().to_string_lossy().parse() {
            Ok(p) => p,
            Err(_) => continue,
        };
        let base = entry.path();
        if !is_mihomo_process(&base) {
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

/// 任意一个 mihomo 进程（用于 TUN capabilities 检查）
fn find_any_mihomo_pid() -> Option<u32> {
    for entry in fs::read_dir("/proc").ok()?.flatten() {
        let pid: u32 = match entry.file_name().to_string_lossy().parse() {
            Ok(p) => p,
            Err(_) => continue,
        };
        if is_mihomo_process(&entry.path()) {
            return Some(pid);
        }
    }
    None
}

/// 进程是否为 mihomo：`comm` 或 `/proc/<pid>/exe` 指向 mihomo
fn is_mihomo_process(base: &Path) -> bool {
    if fs::read_to_string(base.join("comm")).is_ok_and(|c| comm_matches(c.trim())) {
        return true;
    }
    fs::read_link(base.join("exe")).is_ok_and(|exe| exe_matches(&exe.to_string_lossy()))
}

/// 进程名匹配：磁盘执行为 `mihomo`，内存执行为 `memfd:mihomo`（旧内核可能为 fd 数字）
fn comm_matches(comm: &str) -> bool {
    comm == "mihomo" || comm == "mihomo.exe" || comm.starts_with("memfd:mihomo")
}

/// 可执行文件路径匹配：兼容 `/…/mihomo` 与 `/memfd:mihomo (deleted)`
fn exe_matches(exe: &str) -> bool {
    let name = exe.rsplit('/').next().unwrap_or(exe);
    let name = name.strip_suffix(" (deleted)").unwrap_or(name);
    name == "mihomo" || name == "mihomo.exe" || name.starts_with("memfd:mihomo")
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

/// TUN 权限检查：mihomo 进程缺少 CAP_NET_ADMIN/CAP_NET_RAW 时给出提示。
/// 权限通过 coclash 本体的 file capabilities + ambient 继承传递，无需给内核文件 setcap。
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
    let hint = match std::env::current_exe() {
        // Nix store 只读、wrapper 带 cap_setpcap：手动 setcap 要么失败、要么破坏 ambient 传递
        Ok(exe) if is_nix_path(&exe) => {
            "NixOS 请启用 programs.coclash（默认 tun = true，security.wrappers 自动授权）\
             并从 PATH 运行 coclash；不要对 /nix/store 或 /run/wrappers 下的文件手动 setcap"
                .to_string()
        }
        Ok(exe) => format!(
            "请执行一次: sudo setcap cap_net_admin,cap_net_raw+eip {}，再由 coclash 启动 mihomo",
            exe.display()
        ),
        Err(_) => "请给 coclash 授予 CAP_NET_ADMIN/CAP_NET_RAW 后再启动 mihomo".to_string(),
    };
    Some(format!(
        "mihomo(PID={pid})缺少CAP_NET_ADMIN/CAP_NET_RAW，TUN可能起不来。{hint}"
    ))
}

/// 是否处于 Nix 环境（store 只读；/run/wrappers 的 wrapper 由 NixOS 管理）
fn is_nix_path(exe: &Path) -> bool {
    exe.starts_with("/nix/store") || exe.starts_with("/run/wrappers")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_comm_matches() {
        assert!(comm_matches("mihomo"));
        assert!(comm_matches("mihomo.exe"));
        assert!(comm_matches("memfd:mihomo"));
        assert!(!comm_matches("mihomo-tui"));
        assert!(!comm_matches("coclash"));
        assert!(!comm_matches("3"));
    }

    #[test]
    fn test_exe_matches() {
        assert!(exe_matches("/usr/bin/mihomo"));
        assert!(exe_matches("/memfd:mihomo (deleted)"));
        assert!(exe_matches("/home/u/.cache/coclash/bin/abc123/mihomo"));
        assert!(!exe_matches("/usr/bin/coclash"));
        assert!(!exe_matches("/memfd:coclash (deleted)"));
    }

    #[test]
    fn test_is_nix_path() {
        assert!(is_nix_path(Path::new("/nix/store/abc-coclash/bin/coclash")));
        assert!(is_nix_path(Path::new("/run/wrappers/bin/coclash")));
        assert!(!is_nix_path(Path::new("/usr/bin/coclash")));
        assert!(!is_nix_path(Path::new("/home/u/.local/bin/coclash")));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn test_memfd_exec_content_roundtrip() {
        use std::io::{Read, Seek, SeekFrom};

        // 直接验证 memfd_exec 使用的写入路径：解压内容与构建期大小一致
        let raw = embedded::decompress_embedded().unwrap();
        assert!(raw.len() > 1_000_000, "内嵌内核大小异常: {}", raw.len());

        let fd = memfd_exec().unwrap();
        let mut file = std::fs::File::from(fd);
        file.seek(SeekFrom::Start(0)).unwrap();
        let mut readback = Vec::new();
        file.read_to_end(&mut readback).unwrap();
        assert_eq!(readback, raw, "memfd 内容应与解压结果一致");
    }
}
