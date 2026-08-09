use crate::command::mihomo::MihomoStatus;
use crate::config::mihomo_config::MihomoConfig;
use crate::config::node::Node;
use crate::operation_log::OperationLogs;

/// 全局唯一数据源：UI 只读渲染，任务结果经 `App::apply` 更新。
#[derive(Debug)]
pub struct AppState {
    pub nodes: Vec<Node>,
    /// 节点列表光标位置
    pub select: usize,
    /// 当前选中（生效）的代理节点下标
    pub active_node: Option<usize>,
    pub mihomo_status: MihomoStatus,
    pub proxy_running: bool,
    pub is_test_delay: bool,
    pub logs: OperationLogs,
    pub config: MihomoConfig,
}

impl AppState {
    pub fn tun_enabled(&self) -> bool {
        self.config.tun.as_ref().is_some_and(|t| t.enable)
    }

    /// 代理监听地址（以 config.yaml 端口为准）
    pub fn proxy_addr(&self) -> String {
        format!("127.0.0.1:{}", self.config.port)
    }
}
