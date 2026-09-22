//! 心跳：定时同步 mihomo 运行时信息（唯一的状态同步实现）。
//!
//! # 同步内容与节拍
//!
//! - **每 tick（默认 3s）**：端口探测、系统代理状态、`/connections`（累计流量）
//! - **全量（每 10 tick ≈ 30s，或 `sync_now` 强制）**：额外拉取 `/configs`
//!   （模式/端口/TUN/DNS）、`/version`（内核版本）、`/proxies/{group}`（节点与当前节点）
//! - 进程/端口状态：只看控制端口是否可达（廉价 TCP 探测），不区分是否由本程序启动；
//!   端口可达但缓存为停止 → 置运行中并记一次日志，端口不可达 → 停止并清空 runtime
//! - API 失败时指数退避（最多 8 倍周期）；就绪/断开日志各只记一次
//!
//! # 抽象边界
//!
//! IO 只负责产出 `ApiSnapshot`（纯数据），状态变更集中在纯函数
//! `apply_snapshot` / `apply_offline`，可脱离 HTTP 单测。
//!
//! # 并发纪律
//!
//! 每段 `await` 前后各取一次短锁；节点列表按名字保留测速结果，
//! 仅在列表/当前节点变化时更新并记日志。
use super::{Manager, Shared};
use crate::core::mihomo::api::{ConfigsReport, ProxyReport};
use crate::core::mihomo::{self, MihomoStatus};
use crate::core::system_proxy::get_proxy_status;
use crate::error::Error;
use crate::manager::state::{MihomoState, RuntimeInfo, merge_nodes, node_names_changed};
use crate::operation_log::LogType;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;
use tokio::time::{MissedTickBehavior, interval};

/// 全量同步周期（tick 数；快路径 3s × 10 = 30s）
const FULL_EVERY_TICKS: u32 = 10;
/// 失败退避上限：周期最多放大 2^3 = 8 倍
const MAX_BACKOFF_SHIFT: u32 = 3;

/// 本 tick 的 IO 结果（纯数据；None = 本 tick 未拉取该接口）
#[derive(Debug, Default)]
struct ApiSnapshot {
    configs: Option<ConfigsReport>,
    version: Option<String>,
    totals: Option<(u64, u64)>,
    proxies: Option<ProxyReport>,
    /// 任一请求失败时的错误文本（用于退避与首次失败日志）
    error: Option<String>,
    /// 就绪探测（`/configs`）失败：只有它才代表整个 API 不可用
    ready_probe_failed: bool,
}

impl Manager {
    /// 启动心跳（main 在进入事件循环前调用）；间隔为 0 时关闭
    pub fn start_heartbeat(&self) {
        let every = self.settings().heartbeat_interval();
        if every.is_zero() {
            self.log_warn("心跳已关闭（heartbeat_interval_ms=0）：仅手动 r 触发一次性同步");
            return;
        }
        let heartbeat = Heartbeat {
            shared: self.shared().clone(),
            tick: 0,
            failures: 0,
        };
        tokio::spawn(heartbeat.run(every));
    }

    /// 请求立即同步一次：心跳开启时唤醒心跳并强制全量；
    /// 心跳关闭（间隔 0）时退化为一次性同步任务，否则 `r`/启动后的同步会静默失效
    pub fn sync_now(&self) {
        if !self.settings().heartbeat_interval().is_zero() {
            self.shared().sync_now();
            return;
        }
        sync_once_now(self.shared());
    }
}

/// 心跳关闭时的一次性同步任务（强制全量）
fn sync_once_now(shared: &Arc<Shared>) {
    let shared = shared.clone();
    tokio::spawn(async move {
        shared.force_full.store(true, Ordering::Relaxed);
        Heartbeat {
            shared,
            tick: 0,
            failures: 0,
        }
        .sync_once()
        .await;
    });
}

/// 延迟触发一次同步：内核刚启动时端口尚未绑定，立即探测会误判离线并留下
/// 「mihomo 已停止」的误导日志；心跳关闭时同样退化为一次性任务。
pub(crate) fn sync_now_delayed(shared: &Arc<Shared>, delay: Duration) {
    if delay.is_zero() {
        if shared.settings.heartbeat_interval().is_zero() {
            sync_once_now(shared);
        } else {
            shared.sync_now();
        }
        return;
    }
    let shared = shared.clone();
    tokio::spawn(async move {
        tokio::time::sleep(delay).await;
        if shared.settings.heartbeat_interval().is_zero() {
            sync_once_now(&shared);
        } else {
            shared.sync_now();
        }
    });
}

/// 心跳任务：持有节拍与退避计数，IO 与状态应用分离
struct Heartbeat {
    shared: Arc<Shared>,
    tick: u32,
    failures: u32,
}

impl Heartbeat {
    async fn run(mut self, every: Duration) {
        let mut ticker = interval(every);
        ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);
        loop {
            tokio::select! {
                _ = ticker.tick() => {}
                _ = self.shared.sync_notify.notified() => {
                    self.shared.force_full.store(true, Ordering::Relaxed);
                }
            }
            if self.shared.should_quit.load(Ordering::Relaxed) {
                break;
            }
            self.sync_once().await;
        }
    }

    /// 同步一次：端口探测 → 状态跃迁 → 系统代理 → API 快照 → 应用 + 日志
    async fn sync_once(&mut self) {
        self.tick = self.tick.wrapping_add(1);
        let shared = self.shared.clone();

        let ctrl_addr = shared.settings.mihomo_ctrl_addr.clone();
        if !mihomo::process::ctrl_addr_up_async(&ctrl_addr).await {
            let logs = {
                let mut st = shared.lock();
                apply_offline(&mut st.mihomo)
            };
            for (log_type, msg) in logs {
                shared.log(log_type, msg);
            }
            // 离线时清掉退避计数，并预约端口回来后立即全量同步：
            // 否则外部重新启动 mihomo 后，节点/端口可能等最多 8 个 tick 才刷新
            self.failures = 0;
            shared.force_full.store(true, Ordering::Relaxed);
            return;
        }

        // 端口可达但缓存为 Stopped → 直接视为运行中（状态只看端口，不扫描进程表）
        {
            let mut st = shared.lock();
            if st.mihomo.status == MihomoStatus::Stopped {
                st.mihomo.status = MihomoStatus::Running(0);
                st.logs
                    .add_log(LogType::Info, "检测到 mihomo 已在运行".into());
                drop(st);
                shared.mark_redraw();
            }
        }

        // 系统代理状态（Unix 不支持时静默跳过）
        if let Ok((code, _)) = get_proxy_status() {
            let on = code == 1;
            let mut st = shared.lock();
            if st.mihomo.proxy_running != on {
                st.mihomo.proxy_running = on;
                drop(st);
                shared.mark_redraw();
            }
        }

        // API：就绪前每 tick 尝试（用于恢复）；就绪后按退避节奏与全量周期拉取
        let force = shared.force_full.swap(false, Ordering::Relaxed);
        let api_ready = shared.lock().mihomo.runtime.api_ready;
        let backoff = 1u32 << self.failures.min(MAX_BACKOFF_SHIFT);
        if !(force || api_ready || self.tick.is_multiple_of(backoff)) {
            shared.mark_redraw();
            return;
        }
        let full = force || !api_ready || self.tick.is_multiple_of(FULL_EVERY_TICKS);
        let snapshot = fetch_snapshot(&shared, full).await;
        let logs = {
            let mut st = shared.lock();
            apply_snapshot(&mut st.mihomo, &snapshot)
        };
        let first_failure = snapshot.error.is_some() && self.failures == 0;
        match snapshot.error {
            Some(_) => self.failures = self.failures.saturating_add(1),
            None => self.failures = 0,
        }
        for (log_type, msg) in logs {
            shared.log(log_type, msg);
        }
        // 非就绪探测失败（/version、/proxies、/connections）：每次失败连击只提示一次，
        // 就绪探测失败由 apply_snapshot 的断开日志负责，避免重复
        if first_failure
            && !snapshot.ready_probe_failed
            && let Some(e) = &snapshot.error
        {
            shared.log(LogType::Warn, format!("mihomo API 请求失败: {e}"));
        }
        shared.mark_redraw();
    }
}

/// 拉取 API 快照：全量拉 configs/version/proxies，快路径只拉 connections。
/// `configs` 失败即视为 API 不可用（就绪探测），跳过其余请求（避免多次超时）；
/// 其余接口失败只记录错误，不丢弃本轮已成功的数据。
async fn fetch_snapshot(shared: &Arc<Shared>, full: bool) -> ApiSnapshot {
    let mut snapshot = ApiSnapshot::default();
    if full {
        match shared.api.get_configs().await {
            Ok(cfg) => snapshot.configs = Some(cfg),
            Err(e) => {
                snapshot.error = Some(e.to_string());
                snapshot.ready_probe_failed = true;
                return snapshot;
            }
        }
        match shared.api.get_version().await {
            Ok(v) => snapshot.version = Some(v.version),
            Err(e) => record_error(&mut snapshot, e),
        }
        match shared.api.get_proxy().await {
            Ok(p) => snapshot.proxies = Some(p),
            Err(e) => record_error(&mut snapshot, e),
        }
    }
    match shared.api.get_connections().await {
        Ok(c) => snapshot.totals = Some((c.upload_total, c.download_total)),
        Err(e) => record_error(&mut snapshot, e),
    }
    snapshot
}

/// 只保留首个错误文本（用于退避与首次失败日志）
fn record_error(snapshot: &mut ApiSnapshot, e: Error) {
    if snapshot.error.is_none() {
        snapshot.error = Some(e.to_string());
    }
}

/// 纯函数：应用快照并返回待写日志（无 IO / 无锁 / 可单测）。
/// 只有就绪探测失败才清空就绪状态；其余接口失败不影响已成功字段的应用。
fn apply_snapshot(state: &mut MihomoState, snapshot: &ApiSnapshot) -> Vec<(LogType, String)> {
    let mut logs = Vec::new();
    if snapshot.ready_probe_failed {
        if state.runtime.api_ready {
            state.runtime.api_ready = false;
            let e = snapshot.error.as_deref().unwrap_or("未知错误");
            logs.push((LogType::Warn, format!("mihomo API 连接断开: {e}")));
        }
        return logs;
    }
    if let Some(cfg) = &snapshot.configs {
        if !state.runtime.api_ready {
            state.runtime.api_ready = true;
            logs.push((LogType::Info, "mihomo 已就绪，开始同步信息".into()));
        }
        state.runtime.mode = Some(cfg.mode.clone());
        state.runtime.mixed_port = Some(cfg.mixed_port);
        state.runtime.http_port = Some(cfg.port);
        state.runtime.socks_port = Some(cfg.socks_port);
        state.runtime.tun_enabled = cfg.tun.as_ref().map(|t| t.enable);
        state.runtime.dns_enabled = cfg.dns.as_ref().map(|d| d.enable);
    }
    if let Some(version) = &snapshot.version {
        state.runtime.version = Some(version.clone());
    }
    if let Some((upload, download)) = snapshot.totals {
        state.runtime.upload_total = upload;
        state.runtime.download_total = download;
    }
    if let Some(proxy) = &snapshot.proxies {
        let new_nodes = merge_nodes(&state.nodes, &proxy.all);
        let active = proxy.all.iter().position(|n| *n == proxy.now);
        // reload 空窗期 mihomo 只返回 DIRECT 兜底：已有列表时跳过，避免闪空
        let transient_direct =
            proxy.all.len() <= 1 && proxy.all.first().is_some_and(|n| n == "DIRECT");
        if !(transient_direct && !state.nodes.is_empty())
            && (node_names_changed(&state.nodes, &new_nodes) || state.active_node != active)
        {
            state.nodes = new_nodes;
            state.active_node = active;
            logs.push((LogType::Info, "更新代理信息".into()));
        }
    }
    logs
}

/// 纯函数：端口不可达时的离线处理；返回待写日志
fn apply_offline(state: &mut MihomoState) -> Vec<(LogType, String)> {
    let was_running = state.status != MihomoStatus::Stopped;
    // 运行时快照在停止后没有意义：清空，UI 回落到本地配置
    if !(was_running || state.runtime.api_ready || state.runtime.version.is_some()) {
        return vec![];
    }
    state.status = MihomoStatus::Stopped;
    state.runtime = RuntimeInfo::default();
    if was_running {
        vec![(LogType::Info, "mihomo 已停止".into())]
    } else {
        vec![]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::mihomo::api::EnableReport;

    fn snapshot_with_configs() -> ApiSnapshot {
        ApiSnapshot {
            configs: Some(ConfigsReport {
                port: 7892,
                mixed_port: 7890,
                socks_port: 7891,
                mode: "rule".into(),
                tun: Some(EnableReport { enable: true }),
                dns: Some(EnableReport { enable: false }),
            }),
            ..ApiSnapshot::default()
        }
    }

    #[test]
    fn test_ready_logs_once() {
        let mut state = MihomoState::default();
        let first = apply_snapshot(&mut state, &snapshot_with_configs());
        assert!(state.runtime.api_ready);
        assert!(first.iter().any(|(_, m)| m.contains("已就绪")));
        let second = apply_snapshot(&mut state, &snapshot_with_configs());
        assert!(second.is_empty(), "就绪日志只应记录一次");
    }

    #[test]
    fn test_disconnect_logs_once() {
        let mut state = MihomoState::default();
        apply_snapshot(&mut state, &snapshot_with_configs());
        let snapshot = ApiSnapshot {
            error: Some("timeout".into()),
            ready_probe_failed: true,
            ..ApiSnapshot::default()
        };
        let first = apply_snapshot(&mut state, &snapshot);
        assert!(!state.runtime.api_ready);
        assert_eq!(first.len(), 1);
        let second = apply_snapshot(&mut state, &snapshot);
        assert!(second.is_empty(), "断开日志只应记录一次");
    }

    #[test]
    fn test_partial_failure_keeps_successful_data() {
        // /connections 超时（error 非空）不能丢弃同一轮已成功的 /configs、/proxies
        let mut state = MihomoState::default();
        let snapshot = ApiSnapshot {
            configs: Some(ConfigsReport {
                port: 7892,
                mixed_port: 7890,
                socks_port: 7891,
                mode: "rule".into(),
                tun: None,
                dns: None,
            }),
            proxies: Some(ProxyReport {
                alive: true,
                all: vec!["a".into()],
                dialer_proxy: String::new(),
                hidden: false,
                icon: String::new(),
                interface: String::new(),
                name: "Proxy".into(),
                now: "a".into(),
                node_type: "Selector".into(),
            }),
            error: Some("connections timeout".into()),
            ready_probe_failed: false,
            ..ApiSnapshot::default()
        };
        let logs = apply_snapshot(&mut state, &snapshot);
        assert!(state.runtime.api_ready, "只有 /configs 失败才算断开");
        assert_eq!(state.runtime.mode.as_deref(), Some("rule"));
        assert_eq!(state.runtime.http_port, Some(7892));
        assert_eq!(state.nodes.len(), 1);
        assert!(logs.iter().any(|(_, m)| m.contains("已就绪")));
        assert!(
            !logs.iter().any(|(_, m)| m.contains("断开")),
            "部分接口失败不应报断开"
        );
    }

    #[test]
    fn test_fast_snapshot_only_updates_totals() {
        let mut state = MihomoState::default();
        apply_snapshot(&mut state, &snapshot_with_configs());
        state
            .nodes
            .push(crate::manager::state::Node::new("a".into()));
        let fast = ApiSnapshot {
            totals: Some((100, 200)),
            ..ApiSnapshot::default()
        };
        let logs = apply_snapshot(&mut state, &fast);
        assert!(logs.is_empty());
        assert_eq!(state.runtime.upload_total, 100);
        assert_eq!(state.runtime.download_total, 200);
        assert_eq!(state.nodes.len(), 1, "快路径不得触碰节点列表");
        assert_eq!(state.runtime.mode.as_deref(), Some("rule"));
    }

    #[test]
    fn test_full_snapshot_updates_nodes_and_keeps_speed() {
        let mut state = MihomoState::default();
        state.nodes.push(crate::manager::state::Node {
            name: "a".into(),
            speed: "5ms".into(),
        });
        let snapshot = ApiSnapshot {
            configs: Some(ConfigsReport {
                port: 3,
                mixed_port: 1,
                socks_port: 2,
                mode: "global".into(),
                tun: None,
                dns: None,
            }),
            proxies: Some(ProxyReport {
                alive: true,
                all: vec!["a".into(), "b".into()],
                dialer_proxy: String::new(),
                hidden: false,
                icon: String::new(),
                interface: String::new(),
                name: "Proxy".into(),
                now: "b".into(),
                node_type: "Selector".into(),
            }),
            ..ApiSnapshot::default()
        };
        let logs = apply_snapshot(&mut state, &snapshot);
        assert!(logs.iter().any(|(_, m)| m.contains("更新代理信息")));
        assert_eq!(state.nodes.len(), 2);
        assert_eq!(state.nodes[0].speed, "5ms");
        assert_eq!(state.active_node, Some(1));
        assert_eq!(state.runtime.mode.as_deref(), Some("global"));
    }

    #[test]
    fn test_transient_direct_guard() {
        let mut state = MihomoState::default();
        state
            .nodes
            .push(crate::manager::state::Node::new("a".into()));
        let snapshot = ApiSnapshot {
            proxies: Some(ProxyReport {
                alive: true,
                all: vec!["DIRECT".into()],
                dialer_proxy: String::new(),
                hidden: false,
                icon: String::new(),
                interface: String::new(),
                name: "Proxy".into(),
                now: "DIRECT".into(),
                node_type: "Selector".into(),
            }),
            ..ApiSnapshot::default()
        };
        apply_snapshot(&mut state, &snapshot);
        assert_eq!(
            state.nodes.len(),
            1,
            "reload 空窗期的 DIRECT 兜底不应清空列表"
        );
    }

    #[test]
    fn test_offline_resets_runtime_and_logs_once() {
        let mut state = MihomoState {
            status: MihomoStatus::Running(1),
            ..MihomoState::default()
        };
        apply_snapshot(&mut state, &snapshot_with_configs());
        let logs = apply_offline(&mut state);
        assert_eq!(state.status, MihomoStatus::Stopped);
        assert_eq!(state.runtime, RuntimeInfo::default());
        assert_eq!(logs.len(), 1);
        // 状态已 Stopped 且 runtime 已清空：不重复记日志
        assert!(apply_offline(&mut state).is_empty());
    }
}
