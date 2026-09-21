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
    /// 任一请求失败时的错误文本（用于就绪/断开跃迁日志）
    error: Option<String>,
}

impl Manager {
    /// 启动心跳（main 在进入事件循环前调用）；间隔为 0 时关闭
    pub fn start_heartbeat(&self) {
        let every = self.settings().heartbeat_interval();
        if every.is_zero() {
            return;
        }
        let heartbeat = Heartbeat {
            shared: self.shared().clone(),
            tick: 0,
            failures: 0,
        };
        tokio::spawn(heartbeat.run(every));
    }
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
        match snapshot.error {
            Some(_) => self.failures = self.failures.saturating_add(1),
            None => self.failures = 0,
        }
        for (log_type, msg) in logs {
            shared.log(log_type, msg);
        }
        shared.mark_redraw();
    }
}

/// 拉取 API 快照：全量拉 configs/version/proxies，快路径只拉 connections。
/// `configs` 失败即视为 API 不可用，跳过其余请求（避免多次超时）。
async fn fetch_snapshot(shared: &Arc<Shared>, full: bool) -> ApiSnapshot {
    let mut snapshot = ApiSnapshot::default();
    if full {
        match shared.api.get_configs().await {
            Ok(cfg) => snapshot.configs = Some(cfg),
            Err(e) => {
                snapshot.error = Some(e.to_string());
                return snapshot;
            }
        }
        if let Ok(v) = shared.api.get_version().await {
            snapshot.version = Some(v.version);
        }
        if let Ok(p) = shared.api.get_proxy().await {
            snapshot.proxies = Some(p);
        }
    }
    match shared.api.get_connections().await {
        Ok(c) => snapshot.totals = Some((c.upload_total, c.download_total)),
        Err(e) => {
            if snapshot.error.is_none() {
                snapshot.error = Some(e.to_string());
            }
        }
    }
    snapshot
}

/// 纯函数：应用快照并返回待写日志（无 IO / 无锁 / 可单测）
fn apply_snapshot(state: &mut MihomoState, snapshot: &ApiSnapshot) -> Vec<(LogType, String)> {
    let mut logs = Vec::new();
    if let Some(e) = &snapshot.error {
        if state.runtime.api_ready {
            state.runtime.api_ready = false;
            logs.push((LogType::Warn, format!("mihomo API 连接断开: {e}")));
        }
        return logs;
    }
    if snapshot.configs.is_some() && !state.runtime.api_ready {
        state.runtime.api_ready = true;
        logs.push((LogType::Info, "mihomo 已就绪，开始同步信息".into()));
    }
    if let Some(cfg) = &snapshot.configs {
        state.runtime.mode = Some(cfg.mode.clone());
        state.runtime.mixed_port = Some(cfg.mixed_port);
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
            ..ApiSnapshot::default()
        };
        let first = apply_snapshot(&mut state, &snapshot);
        assert!(!state.runtime.api_ready);
        assert_eq!(first.len(), 1);
        let second = apply_snapshot(&mut state, &snapshot);
        assert!(second.is_empty(), "断开日志只应记录一次");
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
