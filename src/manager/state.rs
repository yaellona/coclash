use crate::core::mihomo::MihomoStatus;
use crate::core::config::mihomo_config::MihomoConfig;
use crate::core::config::node::Node;
use crate::operation_log::OperationLogs;

/// 鍏ㄥ眬鍞竴鏁版嵁婧愶細UI 鍙娓叉煋锛屼换鍔＄粨鏋滅粡浠诲姟灞?`TaskEvent::apply` 鏇存柊銆?
#[derive(Debug)]
pub struct AppState {
    pub nodes: Vec<Node>,
    /// 鑺傜偣鍒楄〃鍏夋爣浣嶇疆
    pub select: usize,
    /// 褰撳墠閫変腑锛堢敓鏁堬級鐨勪唬鐞嗚妭鐐逛笅鏍?
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

    /// 浠ｇ悊鐩戝惉鍦板潃锛堜互 config.yaml 绔彛涓哄噯锛?
    pub fn proxy_addr(&self) -> String {
        format!("127.0.0.1:{}", self.config.port)
    }
}
