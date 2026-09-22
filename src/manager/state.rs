use crate::core::config::mihomo_config::MihomoConfig;
use crate::core::mihomo::MihomoStatus;
use crate::operation_log::OperationLogs;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 节点列表展示模型（MihomoState.nodes）：UI 层数据，不属于内核配置
#[derive(Debug, Serialize, Deserialize)]
pub struct Node {
    pub name: String,
    pub speed: String,
}

impl Node {
    pub fn new(name: String) -> Self {
        Self {
            name,
            speed: "-".to_string(),
        }
    }
}

/// 心跳同步的 mihomo 运行时快照（只读展示；来源 mihomo API）
#[derive(Debug, Clone, PartialEq, Default)]
pub struct RuntimeInfo {
    /// 内核版本（/version）
    pub version: Option<String>,
    /// 运行模式（/configs，如 `rule`）
    pub mode: Option<String>,
    /// 混合端口（/configs；未配置时为 0）
    pub mixed_port: Option<u16>,
    /// HTTP 端口（/configs；未配置时为 0）
    pub http_port: Option<u16>,
    /// SOCKS 端口（/configs）
    pub socks_port: Option<u16>,
    /// TUN 开关（/configs）
    pub tun_enabled: Option<bool>,
    /// DNS 开关（/configs）
    pub dns_enabled: Option<bool>,
    /// 累计上行/下行流量（/connections）
    pub upload_total: u64,
    pub download_total: u64,
    /// API 当前可达（用于日志去重与状态展示）
    pub api_ready: bool,
}

/// mihomo 运行时状态：心跳同步 + 进程/任务命令写入。
#[derive(Debug)]
pub struct MihomoState {
    pub status: MihomoStatus,
    pub nodes: Vec<Node>,
    pub active_node: Option<usize>,
    /// 系统代理开关（心跳同步）
    pub proxy_running: bool,
    /// 测速任务进行中（任务守卫）
    pub is_test_delay: bool,
    /// 启停 mihomo 任务进行中（异步任务守卫，避免连按 s 起两个进程）
    pub is_toggling: bool,
    /// 切换节点任务进行中（连按 Enter 时拒绝新任务，见 tasks.rs）
    pub is_switching_node: bool,
    /// 心跳同步的运行时信息（只读展示）
    pub runtime: RuntimeInfo,
}

impl Default for MihomoState {
    fn default() -> Self {
        Self {
            status: MihomoStatus::Stopped,
            nodes: vec![],
            active_node: None,
            proxy_running: false,
            is_test_delay: false,
            is_toggling: false,
            is_switching_node: false,
            runtime: RuntimeInfo::default(),
        }
    }
}

/// 应用共享状态：本地配置（唯一可写源）+ mihomo 运行时 + 操作日志。
///
/// UI 局部状态（节点光标、焦点、滚动位置、编辑缓冲）在各自页面里，
/// 不进这里——页面只读 AppState，修改通过 `Cmd` 交回命令层执行。
#[derive(Debug)]
pub struct AppState {
    pub logs: OperationLogs,
    /// 本地 config.yaml 模型（设置页编辑/落盘的唯一数据源）
    pub config: MihomoConfig,
    pub mihomo: MihomoState,
}

impl AppState {
    pub fn tun_enabled(&self) -> bool {
        self.config.tun.as_ref().is_some_and(|t| t.enable)
    }
    pub fn dns_enabled(&self) -> bool {
        self.config.dns.as_ref().is_some_and(|d| d.enable)
    }
    /// 系统代理地址：优先运行中实际监听端口（mixed-port > http-port），
    /// 否则回落本地配置；端口未知或为 0 时返回 None（不能拿 0 当端口）。
    pub fn proxy_addr(&self) -> Option<String> {
        let rt = &self.mihomo.runtime;
        let port = rt
            .mixed_port
            .filter(|p| *p > 0)
            .or(rt.http_port.filter(|p| *p > 0))
            .or(self.config.mixed_port)
            .or(self.config.port)
            .filter(|p| *p > 0)?;
        Some(format!("127.0.0.1:{port}"))
    }
}

#[cfg(test)]
impl AppState {
    /// 单测夹具：默认配置 + 空运行时状态（页面 handler 测试用）
    pub fn test_fixture() -> Self {
        Self {
            logs: OperationLogs::new(),
            config: MihomoConfig::default_config(),
            mihomo: MihomoState::default(),
        }
    }
}

/// 按新名字列表重建节点，**按名字保留旧 speed**（心跳刷新不能冲掉测速结果）。
pub fn merge_nodes(old: &[Node], names: &[String]) -> Vec<Node> {
    let speeds: HashMap<&str, &str> = old
        .iter()
        .map(|n| (n.name.as_str(), n.speed.as_str()))
        .collect();
    names
        .iter()
        .map(|name| match speeds.get(name.as_str()) {
            Some(s) => Node {
                name: name.clone(),
                speed: (*s).to_string(),
            },
            None => Node::new(name.clone()),
        })
        .collect()
}

/// 名字列表是否发生变化（speed 变化不算，避免心跳日志刷屏）
pub fn node_names_changed(old: &[Node], new: &[Node]) -> bool {
    old.len() != new.len() || old.iter().zip(new.iter()).any(|(a, b)| a.name != b.name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(name: &str, speed: &str) -> Node {
        Node {
            name: name.to_string(),
            speed: speed.to_string(),
        }
    }

    #[test]
    fn test_merge_nodes_preserves_speed() {
        let old = vec![node("a", "123ms"), node("b", "wait"), node("c", "-")];
        let names = vec!["a".to_string(), "c".to_string(), "d".to_string()];
        let merged = merge_nodes(&old, &names);
        assert_eq!(merged.len(), 3);
        assert_eq!(merged[0].speed, "123ms");
        assert_eq!(merged[1].speed, "-");
        assert_eq!(merged[2].speed, "-");
    }

    #[test]
    fn test_node_names_changed() {
        let a = vec![node("a", "1ms"), node("b", "2ms")];
        assert!(!node_names_changed(&a, &a));
        let speed_only = vec![node("a", "999ms"), node("b", "2ms")];
        assert!(!node_names_changed(&a, &speed_only));
        let renamed = vec![node("a", "1ms"), node("x", "2ms")];
        assert!(node_names_changed(&a, &renamed));
        assert!(node_names_changed(&a, &[]));
    }

    #[test]
    fn test_proxy_addr_prefers_runtime() {
        let mut state = AppState::test_fixture();
        assert_eq!(
            state.proxy_addr(),
            Some(format!("127.0.0.1:{}", state.config.port.unwrap()))
        );
        // 运行时 mixed-port=0（未启用混合端口）不能覆盖本地 HTTP 端口
        state.mihomo.runtime.mixed_port = Some(0);
        assert_eq!(
            state.proxy_addr(),
            Some(format!("127.0.0.1:{}", state.config.port.unwrap()))
        );
        state.mihomo.runtime.mixed_port = Some(12345);
        assert_eq!(state.proxy_addr().as_deref(), Some("127.0.0.1:12345"));
        // 完全无端口配置时不得返回 127.0.0.1:0
        state.config.port = None;
        state.mihomo.runtime.mixed_port = Some(0);
        assert_eq!(state.proxy_addr(), None);
    }
}
