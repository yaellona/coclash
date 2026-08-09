//! 异步任务：只产生类型化结果消息 `TaskResult`，由主线程 `App::apply` 消费。
//! 任务不持有 Manager/Window，也不直接修改任何 UI 状态。
use crate::command::mihomo::api::ProxyReport;
use crate::error::Error;
use std::collections::HashMap;

#[derive(Debug)]
pub enum TaskResult {
    /// 启动 mihomo 后端口是否就绪
    MihomoReady {
        ready: bool,
    },
    NodesFetched {
        result: Result<ProxyReport, Error>,
    },
    DelaysFetched {
        result: Result<HashMap<String, u32>, Error>,
    },
    NodeSwitched {
        name: String,
        result: Result<(), Error>,
    },
    ConfigReloaded {
        result: Result<(), Error>,
    },
    ProviderNameFetched {
        url: String,
        result: Result<String, Error>,
    },
}
