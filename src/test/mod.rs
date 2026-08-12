//! 集成/活体测试。
//!
//! 纯单元测试已归位到各自模块（各文件的 `#[cfg(test)] mod tests`）；
//! 本文件只保留需要真实系统环境的测试：`live_detect_status_across_runs`
//! 要求系统里有一个正在运行的 mihomo，默认 `#[ignore]`，
//! 显式用 `cargo test -- --ignored` 运行。
use crate::settings::Settings;

/// 活体验证（默认跳过，需显式运行）：
/// 模拟「A 进程启动了 mihomo 并写了 pidfile，B 进程再探测」的跨运行场景，
/// 要求系统里确实有一个 mihomo 在运行且端口 9090 可达。
#[cfg(windows)]
#[test]
#[ignore = "活体系统检查，需真实 mihomo 在运行"]
fn live_detect_status_across_runs() {
    use crate::constants::PID_FILE;
    use crate::core::mihomo::{MihomoStatus, detect_status};

    let config_dir = dirs::config_dir().unwrap().join(crate::constants::CONFIG_DIR_NAME);
    let pidfile = config_dir.join(PID_FILE);

    let out = std::process::Command::new("powershell")
        .args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            "(Get-Process -Name 'mihomo*' -ErrorAction SilentlyContinue | Select-Object -First 1).Id",
        ])
        .output()
        .expect("运行 powershell 失败");
    let pid: u32 = String::from_utf8_lossy(&out.stdout)
        .trim()
        .parse()
        .expect("未检测到运行中的 mihomo，先启动 mihomo 再跑本测试");

    std::fs::write(&pidfile, pid.to_string()).unwrap();
    let status = detect_status(&Settings::default(), &config_dir);
    let pidfile_still_here = pidfile.exists();
    let _ = std::fs::remove_file(&pidfile);

    assert_eq!(
        status,
        MihomoStatus::RunningByUs(pid),
        "应判定为 RunningByUs 而非 External/Stopped"
    );
    assert!(pidfile_still_here, "RunningByUs 时不得清掉 pidfile");
}
