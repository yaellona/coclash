use crate::core::config::mihomo_config::MihomoConfig;
use crate::core::mihomo::MihomoStatus;
use crate::operation_log::OperationLogs;
use serde::{Deserialize, Serialize};

/// 节点列表展示模型（AppState.nodes）：UI 层数据，不属于内核配置
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

#[derive(Debug)]
pub struct AppState {
    pub nodes: Vec<Node>,
    pub select: usize,
    pub active_node: Option<usize>,
    pub mihomo_status: MihomoStatus,
    pub proxy_running: bool,
    pub is_test_delay: bool,
    /// 切换节点任务进行中（连按 Enter 时拒绝新任务，见 task.rs）
    pub is_switching_node: bool,
    pub logs: OperationLogs,
    pub config: MihomoConfig,
}

impl AppState {
    pub fn tun_enabled(&self) -> bool {
        self.config.tun.as_ref().is_some_and(|t| t.enable)
    }
    pub fn dns_enabled(&self) -> bool {
        self.config.dns.as_ref().is_some_and(|d| d.enable)
    }
    /// 节点光标移动（环绕）。UI 状态修改的唯一入口，禁止窗口直接字段赋值。
    pub fn navigate(&mut self, step: i32) {
        let len = self.nodes.len();
        if len == 0 {
            return;
        }
        self.select = (self.select as i32 + step).rem_euclid(len as i32) as usize;
    }
    pub fn proxy_addr(&self) -> String {
        format!("127.0.0.1:{}", self.config.port)
    }
}
