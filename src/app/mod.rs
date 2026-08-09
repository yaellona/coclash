//! 应用核心：`App` 持有状态与依赖，负责同步命令、异步结果回灌。
pub mod event;
pub mod state;
pub mod tasks;

use crate::command::mihomo::{self, ApiClient, MihomoStatus};
use crate::command::system_proxy::{disable_proxy, enable_proxy, get_proxy_status};
use crate::config::mihomo_config::MihomoConfig;
use crate::config::node::Node;
use crate::constants::{CONFIG_DIR_NAME, CONFIG_FILE, SETTINGS_FILE};
use crate::error::Error;
use crate::operation_log::{LogType, OperationLogs};
use crate::settings::Settings;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::mpsc;

use state::AppState;
use tasks::TaskResult;

pub struct App {
    pub state: AppState,
    pub settings: Settings,
    pub config_path: PathBuf,
    api: Arc<ApiClient>,
    tx: mpsc::Sender<TaskResult>,
    rx: mpsc::Receiver<TaskResult>,
    pub should_quit: bool,
}

impl App {
    /// 完成所有可能失败的 IO 初始化（在进入 raw mode 之前调用）。
    pub fn new() -> Result<Self, Error> {
        let config_dir = dirs::config_dir()
            .ok_or_else(|| Error::Config("无法获取配置目录".to_string()))?
            .join(CONFIG_DIR_NAME);
        std::fs::create_dir_all(&config_dir)?;

        let settings_path = config_dir.join(SETTINGS_FILE);
        let config_path = config_dir.join(CONFIG_FILE);
        let settings = Settings::load_or_create(&settings_path);

        let config = MihomoConfig::read_from_file(&config_path).unwrap_or_else(|_| {
            let c = MihomoConfig::default_config();
            let _ = c.write_to_path(&config_path);
            c
        });

        let api = Arc::new(ApiClient::new(&settings, config.group_name())?);
        let (tx, rx) = mpsc::channel::<TaskResult>(settings.channel_capacity);

        let status = mihomo::detect_status(&settings, &config_dir);
        let proxy_running = get_proxy_status().is_ok_and(|(v, _)| v == 1);

        Ok(Self {
            state: AppState {
                nodes: vec![],
                select: 0,
                active_node: None,
                mihomo_status: status,
                proxy_running,
                is_test_delay: false,
                logs: OperationLogs::new(),
                config,
            },
            settings,
            config_path,
            api,
            tx,
            rx,
            should_quit: false,
        })
    }

    pub fn config_dir(&self) -> &Path {
        self.config_path.parent().unwrap_or(Path::new("."))
    }

    // ===== 日志 =====

    pub fn log(&mut self, msg: impl Into<String>) {
        self.state.logs.add_log(LogType::Info, msg.into());
    }

    pub fn log_err(&mut self, e: impl std::fmt::Display) {
        self.state.logs.add_log(LogType::Error, e.to_string());
    }

    pub fn log_warn(&mut self, msg: impl Into<String>) {
        self.state.logs.add_log(LogType::Warn, msg.into());
    }

    // ===== 任务 spawn =====

    fn spawn<F>(&self, fut: F)
    where
        F: std::future::Future<Output = TaskResult> + Send + 'static,
    {
        let tx = self.tx.clone();
        tokio::spawn(async move {
            let _ = tx.send(fut.await).await;
        });
    }

    pub fn drain_tasks(&mut self) {
        while let Ok(result) = self.rx.try_recv() {
            self.apply(result);
        }
    }

    // ===== 命令（同步，按键直接触发） =====

    pub fn start_mihomo(&mut self) {
        match mihomo::start_mihomo(&self.settings, &self.config_path, self.state.tun_enabled()) {
            Ok((pid, binary)) => {
                self.state.mihomo_status = MihomoStatus::RunningByUs(pid);
                self.log(format!(
                    "mihomo 已启动 (PID {pid}, {}: {})",
                    binary.source.label(),
                    binary.cmd
                ));
                self.wait_for_start();
            }
            Err(e) => self.log_err(e),
        }
    }

    pub fn stop_mihomo(&mut self) {
        let config_dir = self.config_dir().to_path_buf();
        match mihomo::stop_mihomo(&self.settings, &config_dir) {
            Ok(()) => {
                self.state.mihomo_status = MihomoStatus::Stopped;
                self.log("已停止mihomo");
            }
            Err(e) => self.log_err(e),
        }
    }

    pub fn toggle_mihomo(&mut self) {
        let config_dir = self.config_dir().to_path_buf();
        match mihomo::detect_status(&self.settings, &config_dir) {
            MihomoStatus::Stopped => self.start_mihomo(),
            _ => self.stop_mihomo(),
        }
    }

    pub fn toggle_system_proxy(&mut self) {
        let is_enabled = get_proxy_status()
            .map(|(code, _)| code == 1)
            .unwrap_or(false);
        self.state.proxy_running = !is_enabled;
        if is_enabled {
            match disable_proxy() {
                Ok(()) => self.log("关闭系统代理"),
                Err(e) => self.log_err(e),
            }
        } else {
            let addr = self.state.proxy_addr();
            match enable_proxy(&addr) {
                Ok(()) => self.log("开启系统代理"),
                Err(e) => self.log_err(e),
            }
        }
    }

    pub fn toggle_tun(&mut self) {
        let new_state = !self.state.tun_enabled();
        match self
            .state
            .config
            .set_tun_enabled(new_state, &self.config_path)
        {
            Ok(()) => {
                self.log(format!("TUN已{}", if new_state { "开启" } else { "关闭" }));
                #[cfg(unix)]
                if new_state && let Some(warn) = mihomo::tun_capability_warning() {
                    self.log_warn(warn);
                }
                self.reload_config();
            }
            Err(e) => self.log_err(e),
        }
    }

    pub fn switch_provider(&mut self, name: String) {
        match self
            .state
            .config
            .prepare_switch_provider(&name, &self.config_path)
        {
            Ok(()) => {
                self.log("正在切换代理商...");
                self.reload_config();
            }
            Err(e) => self.log_err(e),
        }
    }

    /// 按当前 select 切换节点
    pub fn switch_node(&mut self, index: usize) {
        let Some(node) = self.state.nodes.get(index) else {
            return;
        };
        self.state.active_node = Some(index);
        let name = node.name.clone();
        let api = self.api.clone();
        self.spawn(async move {
            let result = api.switch_node(&name).await;
            TaskResult::NodeSwitched { name, result }
        });
    }

    pub fn start_delay_test(&mut self) {
        if self.state.is_test_delay {
            self.log_warn("已经在测速了!");
            return;
        }
        self.state.is_test_delay = true;
        for node in &mut self.state.nodes {
            node.speed = "wait".to_string();
        }
        let api = self.api.clone();
        self.spawn(async move {
            TaskResult::DelaysFetched {
                result: api.fetch_delays().await,
            }
        });
    }

    pub fn load_nodes(&self) {
        let api = self.api.clone();
        self.spawn(async move {
            TaskResult::NodesFetched {
                result: api.get_proxy().await,
            }
        });
    }

    pub fn reload_config(&self) {
        let api = self.api.clone();
        let path = self.config_path.clone();
        self.spawn(async move {
            TaskResult::ConfigReloaded {
                result: api.reload_config(&path).await,
            }
        });
    }

    /// 添加订阅：先异步探测名称，回传后在主线程写配置
    pub fn insert_sub(&mut self, url: String) {
        self.log("正在验证URL...");
        let api = self.api.clone();
        let fetch_url = url.clone();
        self.spawn(async move {
            TaskResult::ProviderNameFetched {
                url,
                result: api.get_provider_name(&fetch_url).await,
            }
        });
    }

    fn wait_for_start(&self) {
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
            TaskResult::MihomoReady { ready }
        });
    }

    // ===== 任务结果回灌 =====

    fn apply(&mut self, result: TaskResult) {
        match result {
            TaskResult::MihomoReady { ready } => {
                if ready {
                    self.log("mihomo 已就绪，正在拉取节点");
                    self.load_nodes();
                } else {
                    self.log_warn(
                        "进程已启动但端口未就绪（启动可能较慢或失败），可按 s 停止后重试",
                    );
                }
            }
            TaskResult::NodesFetched { result } => match result {
                Ok(proxy) => {
                    self.state.nodes = vec![];
                    self.state.select = 0;
                    self.state.active_node = None;
                    for (index, node) in proxy.all.into_iter().enumerate() {
                        if node == proxy.now {
                            self.state.active_node = Some(index);
                            self.state.select = index;
                        }
                        self.state.nodes.push(Node::new(node));
                    }
                    self.log("更新代理信息");
                }
                Err(e) => self.log_err(e),
            },
            TaskResult::DelaysFetched { result } => {
                self.state.is_test_delay = false;
                match result {
                    Ok(map) => {
                        for node in &mut self.state.nodes {
                            node.speed = match map.get(&node.name) {
                                Some(&d) => format!("{d}ms"),
                                None => "-".to_string(),
                            };
                        }
                        self.log("测速完成");
                    }
                    Err(e) => self.log_err(e),
                }
            }
            TaskResult::NodeSwitched { name, result } => match result {
                Ok(()) => self.log(format!("切换节点：{name}")),
                Err(e) => self.log_err(e),
            },
            TaskResult::ConfigReloaded { result } => match result {
                Ok(()) => {
                    self.log("重置配置成功");
                    self.load_nodes();
                }
                Err(e) => self.log_err(e),
            },
            TaskResult::ProviderNameFetched { url, result } => {
                let name = match result {
                    Ok(name) => name,
                    Err(e) => {
                        let n = self
                            .state
                            .config
                            .proxy_providers
                            .as_ref()
                            .map(|p| p.len())
                            .unwrap_or(0)
                            + 1;
                        let fallback = format!("订阅{n}");
                        self.log_warn(format!("{e}，使用默认名称 {fallback}"));
                        fallback
                    }
                };
                match self
                    .state
                    .config
                    .insert_sub(url, name.clone(), &self.config_path)
                {
                    Ok(()) => {
                        self.log(format!("插入代理商：{name}"));
                        self.reload_config();
                    }
                    Err(e) => self.log_err(e),
                }
            }
        }
    }
}
