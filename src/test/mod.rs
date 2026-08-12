//! 集成/活体测试。
//!
//! 纯单元测试已归位到各自模块（各文件的 `#[cfg(test)] mod tests`）；
//! 本文件只保留需要真实系统环境的测试：`live_detect_status_across_runs`
//! 要求系统里有一个正在运行的 mihomo，默认 `#[ignore]`，
//! 显式用 `cargo test -- --ignored` 运行。

/// 活体验证（默认跳过，需显式运行）：
/// 要求系统里确实有一个命令行含 coclash config_dir 的 mihomo 在运行。
/// 判定不依赖任何文件记录，纯进程表扫描。
#[cfg(windows)]
#[test]
#[ignore = "活体系统检查，需真实 mihomo 在运行"]
fn live_detect_status_across_runs() {
    use crate::core::mihomo::{MihomoStatus, detect_status};
    use crate::settings::Settings;

    let config_dir = dirs::config_dir().unwrap().join(crate::constants::CONFIG_DIR_NAME);

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

    let status = detect_status(&Settings::default(), &config_dir);

    assert_eq!(
        status,
        MihomoStatus::RunningByUs(pid),
        "应判定为 RunningByUs 而非 External/Stopped"
    );
}
