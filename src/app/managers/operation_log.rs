use crate::operation_log::{LogType, OperationLogs};

/// 操作日志管理器：日志数据 + 短方法
#[derive(Debug)]
pub struct OperationLogManager {
    pub items: OperationLogs,
}

impl OperationLogManager {
    pub fn add(&mut self, log_type: LogType, msg: String) {
        self.items.add_log(log_type, msg);
    }
}
