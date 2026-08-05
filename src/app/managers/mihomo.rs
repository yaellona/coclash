use crate::command::mihomo::MihomoStatus;

/// mihomo 运行时状态管理器
#[derive(Debug)]
pub struct MihomoManager {
    pub status: MihomoStatus,
    pub proxy_running: bool,
    pub tun_enabled: bool,
    pub active_node: Option<usize>,
    pub is_test_delay: bool,
}
