//! 集成/活体测试。
//!
//! 纯单元测试已归位到各自模块（各文件的 `#[cfg(test)] mod tests`）；
//! 本文件只保留需要真实系统环境的测试，默认 `#[ignore]`，显式用
//! `cargo test -- --ignored` 运行：
//!
//! - `live_embedded_mihomo_extract`：内嵌 mihomo 释放到缓存并可执行
//! - `live_embedded_mihomo_start_stop`：用内嵌 mihomo 真实启动/停止（临时目录 + 独立端口）
//! - `live_heartbeat_sync`：要求本机 external-controller（默认 127.0.0.1:9090）有 mihomo

/// 活体验证（默认跳过，需显式运行）：
/// 要求本机 external-controller 有可访问的 mihomo；验证心跳能同步
/// `/configs`、`/version`、`/connections`、`/proxies/{group}` 四类信息。
#[tokio::test]
#[ignore = "活体系统检查，需真实 mihomo 在运行"]
async fn live_heartbeat_sync() {
    use crate::manager::Manager;
    use std::time::Duration;

    let manager = Manager::new().expect("创建 Manager 失败");
    manager.start_heartbeat();
    manager.sync_now();

    for _ in 0..30 {
        tokio::time::sleep(Duration::from_millis(100)).await;
        let st = manager.state_lock();
        let runtime = &st.mihomo.runtime;
        if runtime.api_ready {
            assert!(runtime.mode.is_some(), "runtime.mode 应已同步");
            assert!(runtime.mixed_port.is_some(), "runtime.mixed_port 应已同步");
            assert!(runtime.version.is_some(), "runtime.version 应已同步");
            return;
        }
    }
    panic!("心跳未在 3 秒内同步到 API 信息（mihomo 未运行或端口不可达？）");
}

/// 活体验证（默认跳过，需显式运行）：
/// 内嵌 mihomo 能释放到缓存目录、可执行，且版本与编译期一致。
#[test]
#[ignore = "会写入缓存目录，需显式运行"]
fn live_embedded_mihomo_extract() {
    use crate::core::mihomo::embedded;
    use std::process::Command;

    let path = embedded::ensure_extracted().expect("释放内嵌 mihomo 失败");
    let meta = std::fs::metadata(&path).expect("释放后的文件不存在");
    assert!(
        meta.len() > 1_000_000,
        "内嵌 mihomo 大小异常: {}",
        meta.len()
    );

    let out = Command::new(&path)
        .arg("-v")
        .output()
        .expect("执行内嵌 mihomo 失败");
    let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
    assert!(
        text.contains(embedded::VERSION),
        "内嵌 mihomo 版本不符：期望 {}，实际 {text}",
        embedded::VERSION
    );
    println!("extracted: {} ({})", path.display(), text);
}

/// 活体验证（默认跳过，需显式运行）：
/// 用内嵌 mihomo 在临时目录真实启动一次，验证状态判定与停止。
#[test]
#[ignore = "会启动/停止真实 mihomo 进程，需显式运行"]
fn live_embedded_mihomo_start_stop() {
    use crate::core::mihomo::{self, MihomoStatus};
    use crate::settings::Settings;
    use std::time::{Duration, Instant};

    let dir = tempfile::TempDir::new().unwrap();
    let config_path = dir.path().join("config.yaml");
    std::fs::write(
        &config_path,
        "mixed-port: 17890\nexternal-controller: 127.0.0.1:19090\nmode: rule\nlog-level: silent\n",
    )
    .unwrap();

    let settings = Settings {
        mihomo_ctrl_addr: "127.0.0.1:19090".to_string(),
        ..Settings::default()
    };
    let (pid, bin) =
        mihomo::start_mihomo(&settings, &config_path, false).expect("启动内嵌 mihomo 失败");

    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline && !mihomo::process::is_port_up(&settings) {
        std::thread::sleep(Duration::from_millis(200));
    }
    assert!(
        mihomo::process::is_port_up(&settings),
        "内嵌 mihomo 未在 10s 内就绪: {}",
        bin.display()
    );
    assert!(
        mihomo::process::is_pid_alive(pid),
        "启动的 mihomo 进程应存活"
    );
    // 状态只看端口：可达即 Running（PID 由启动路径记录，探测不关心）
    assert_eq!(mihomo::detect_status(&settings), MihomoStatus::Running(0));

    mihomo::stop_mihomo(&settings).expect("停止内嵌 mihomo 失败");
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline && mihomo::process::is_port_up(&settings) {
        std::thread::sleep(Duration::from_millis(100));
    }
    assert!(!mihomo::process::is_port_up(&settings), "停止后端口仍可达");
}
