#[derive(Debug, Clone, PartialEq)]
pub enum LogType {
    Info,
    Warn,
    Error,
}
impl LogType {
    pub fn as_str(&self) -> &'static str {
        match self {
            LogType::Info => "Info",
            LogType::Warn => "Warn",
            LogType::Error => "Error",
        }
    }
}
const MAX_LOGS: usize = 500;

#[derive(Debug)]
pub struct OperationLogs {
    logs: Vec<OperationLog>,
}

impl OperationLogs {
    pub fn new() -> Self {
        Self { logs: vec![] }
    }
    pub fn len(&self) -> usize {
        self.logs.len()
    }
    pub fn is_empty(&self) -> bool {
        self.logs.is_empty()
    }
    pub fn add_log(&mut self, log_type: LogType, msg: String) {
        self.logs.push(OperationLog::new(log_type, msg));
        if self.logs.len() > MAX_LOGS {
            let extra = self.logs.len() - MAX_LOGS;
            self.logs.drain(0..extra);
        }
    }
    pub fn iter(&self) -> impl Iterator<Item = &OperationLog> {
        self.logs.iter()
    }
}

impl Default for OperationLogs {
    fn default() -> Self {
        Self::new()
    }
}
#[derive(Debug, Clone)]
pub struct OperationLog {
    pub log_type: LogType,
    pub msg: String,
}

impl OperationLog {
    fn new(log_type: LogType, msg: String) -> Self {
        Self { log_type, msg }
    }
}
