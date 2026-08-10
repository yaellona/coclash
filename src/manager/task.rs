//! 任务层：任务发起（`TaskBus`）与结果回灌（`TaskEvent::apply`）自成一体。
//! `Manager` 只做一行转发，不感知任何任务细节。
use crate::core::config::node::Node;
use crate::core::mihomo::{self, ApiClient};
use crate::core::mihomo::api::ProxyReport;
use crate::error::Error;
use crate::manager::state::AppState;
use crate::operation_log::LogType;
use crate::settings::Settings;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::mpsc;

/// 任务完成事件（C# 事件模型：任务执行 → 触发事件 → 挂载的 apply 自动回灌）
#[derive(Debug)]
pub enum TaskEvent {
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

impl TaskEvent {
    /// 结果回灌：状态更新 + 操作记录 + 续发任务，全部在 TUI 之下完成
    pub fn apply(self, state: &mut AppState, bus: &TaskBus) {
        match self {
            TaskEvent::MihomoReady { ready } => {
                if ready {
                    state.logs.add_log(LogType::Info, "mihomo 已就绪，正在拉取节点".into());
                    bus.load_nodes();
                } else {
                    state.logs.add_log(
                        LogType::Warn,
                        "进程已启动但端口未就绪（启动可能较慢或失败），可按 s 停止后重试".into(),
                    );
                }
            }
            TaskEvent::NodesFetched { result } => match result {
                Ok(proxy) => {
                    state.nodes = vec![];
                    state.select = 0;
                    state.active_node = None;
                    for (index, node) in proxy.all.into_iter().enumerate() {
                        if node == proxy.now {
                            state.active_node = Some(index);
                            state.select = index;
                        }
                        state.nodes.push(Node::new(node));
                    }
                    state.logs.add_log(LogType::Info, "更新代理信息".into());
                }
                Err(e) => state.logs.add_log(LogType::Error, e.to_string()),
            },
            TaskEvent::DelaysFetched { result } => {
                state.is_test_delay = false;
                match result {
                    Ok(map) => {
                        for node in &mut state.nodes {
                            node.speed = match map.get(&node.name) {
                                Some(&d) => format!("{d}ms"),
                                None => "-".to_string(),
                            };
                        }
                        state.logs.add_log(LogType::Info, "测速完成".into());
                    }
                    Err(e) => state.logs.add_log(LogType::Error, e.to_string()),
                }
            }
            TaskEvent::NodeSwitched { name, result } => match result {
                Ok(()) => state
                    .logs
                    .add_log(LogType::Info, format!("切换节点：{name}")),
                Err(e) => state.logs.add_log(LogType::Error, e.to_string()),
            },
            TaskEvent::ConfigReloaded { result } => match result {
                Ok(()) => {
                    state.logs.add_log(LogType::Info, "重置配置成功".into());
                    bus.load_nodes();
                }
                Err(e) => state.logs.add_log(LogType::Error, e.to_string()),
            },
            TaskEvent::ProviderNameFetched { url, result } => {
                let name = match result {
                    Ok(name) => name,
                    Err(e) => {
                        let n = state
                            .config
                            .proxy_providers
                            .as_ref()
                            .map(|p| p.len())
                            .unwrap_or(0)
                            + 1;
                        let fallback = format!("订阅{n}");
                        state.logs.add_log(
                            LogType::Warn,
                            format!("{e}，使用默认名称 {fallback}"),
                        );
                        fallback
                    }
                };
                match state.config.insert_sub(url, name.clone(), &bus.config_path) {
                    Ok(()) => {
                        state
                            .logs
                            .add_log(LogType::Info, format!("插入代理商：{name}"));
                        bus.reload_config();
                    }
                    Err(e) => state.logs.add_log(LogType::Error, e.to_string()),
                }
            }
        }
    }
}

/// 任务发起器：持有 api/设置/配置路径与发送端，`Manager` 只做转发
pub struct TaskBus {
    api: Arc<ApiClient>,
    settings: Settings,
    config_path: PathBuf,
    tx: mpsc::Sender<TaskEvent>,
}

impl TaskBus {
    pub fn new(
        settings: &Settings,
        config_path: PathBuf,
        group_name: &str,
        channel_capacity: usize,
    ) -> Result<(Self, mpsc::Receiver<TaskEvent>), Error> {
        let api = Arc::new(ApiClient::new(settings, group_name)?);
        let (tx, rx) = mpsc::channel::<TaskEvent>(channel_capacity);
        Ok((
            Self {
                api,
                settings: settings.clone(),
                config_path,
                tx,
            },
            rx,
        ))
    }

    fn spawn(&self, fut: impl std::future::Future<Output = TaskEvent> + Send + 'static) {
        let tx = self.tx.clone();
        tokio::spawn(async move {
            let _ = tx.send(fut.await).await;
        });
    }

    pub fn load_nodes(&self) {
        let api = self.api.clone();
        self.spawn(async move {
            TaskEvent::NodesFetched {
                result: api.get_proxy().await,
            }
        });
    }

    /// 测速：守卫 + 预置 waiting 状态，然后发起异步测速
    pub fn start_delay_test(&self, state: &mut AppState) {
        if state.is_test_delay {
            state.logs.add_log(LogType::Warn, "已经在测速了!".into());
            return;
        }
        state.is_test_delay = true;
        for node in &mut state.nodes {
            node.speed = "wait".to_string();
        }
        let api = self.api.clone();
        self.spawn(async move {
            TaskEvent::DelaysFetched {
                result: api.fetch_delays().await,
            }
        });
    }

    pub fn switch_node(&self, state: &mut AppState, index: usize) {
        let Some(node) = state.nodes.get(index) else {
            return;
        };
        state.active_node = Some(index);
        let name = node.name.clone();
        let api = self.api.clone();
        self.spawn(async move {
            let result = api.switch_node(&name).await;
            TaskEvent::NodeSwitched { name, result }
        });
    }

    pub fn reload_config(&self) {
        let api = self.api.clone();
        let path = self.config_path.clone();
        self.spawn(async move {
            TaskEvent::ConfigReloaded {
                result: api.reload_config(&path).await,
            }
        });
    }

    pub fn insert_sub(&self, state: &mut AppState, url: String) {
        state.logs.add_log(LogType::Info, "正在验证URL...".into());
        let api = self.api.clone();
        let fetch_url = url.clone();
        self.spawn(async move {
            TaskEvent::ProviderNameFetched {
                url,
                result: api.get_provider_name(&fetch_url).await,
            }
        });
    }

    /// 启动后就绪探测：轮询控制端口直到就绪或超时
    pub fn wait_for_start(&self) {
        let ctrl_addr = self.settings.mihomo_ctrl_addr.clone();
        let attempts = self.settings.provider_retry.max(1);
        let interval = self.settings.provider_retry_interval();
        self.spawn(async move {
            let mut ready = false;
            for _ in 0..attempts {
                if mihomo::process::ctrl_addr_up_async(&ctrl_addr).await {
                    ready = true;
                    break;
                }
                tokio::time::sleep(interval).await;
            }
            if !ready {
                ready = mihomo::process::ctrl_addr_up_async(&ctrl_addr).await;
            }
            TaskEvent::MihomoReady { ready }
        });
    }
}
