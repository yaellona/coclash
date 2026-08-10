//! 管理器层：持有数据（AppState）与任务层（TaskBus），提供命令面；不依赖 tui。
pub mod state;
pub mod task;

use crate::constants::{CONFIG_DIR_NAME, CONFIG_FILE, SETTINGS_FILE};
use crate::core::config::mihomo_config::MihomoConfig;
use crate::core::mihomo::{self, MihomoStatus};
use crate::core::system_proxy::{disable_proxy, enable_proxy, get_proxy_status};
use crate::error::Error;
use crate::operation_log::{LogType, OperationLogs};
use crate::settings::Settings;
use std::path::{Path, PathBuf};
use tokio::sync::mpsc;

use state::AppState;
use task::{TaskBus, TaskEvent};

/// 管理器：数据与任务的中枢，TUI 只通过它发命令与读状态。
pub struct Manager {
    pub state: AppState,
    pub settings: Settings,
    pub config_path: PathBuf,
    bus: TaskBus,
    rx: mpsc::Receiver<TaskEvent>,
    pub should_quit: bool,
}

impl Manager {
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

        let (bus, rx) = TaskBus::new(
            &settings,
            config_path.clone(),
            config.group_name(),
            settings.channel_capacity,
        )?;

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
            bus,
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

    // ===== 任务回灌（任务逻辑在 manager/task.rs） =====

    pub fn drain_tasks(&mut self) {
        while let Ok(event) = self.rx.try_recv() {
            event.apply(&mut self.state, &self.bus);
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
                self.bus.wait_for_start();
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

    // ===== 任务发起（一行转发，任务逻辑在 manager/task.rs） =====

    pub fn switch_node(&mut self, index: usize) {
        self.bus.switch_node(&mut self.state, index);
    }

    pub fn start_delay_test(&mut self) {
        self.bus.start_delay_test(&mut self.state);
    }

    pub fn load_nodes(&self) {
        self.bus.load_nodes();
    }

    pub fn reload_config(&self) {
        self.bus.reload_config();
    }

    pub fn insert_sub(&mut self, url: String) {
        self.bus.insert_sub(&mut self.state, url);
    }
}
