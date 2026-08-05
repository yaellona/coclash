use crate::app::managers::{ConfigManager, MihomoManager, OperationLogManager, TaskChannel};
use crate::app::ui::WindowCtx;
use crate::app::{WindowId, ui};
use crate::command::mihomo::{self, MihomoStatus};
use crate::command::system_proxy::{disable_proxy, enable_proxy, get_proxy_status};
use crate::config::mihomo_config::MihomoConfig;
use crate::constants::{CONFIG_DIR_NAME, CONFIG_FILE, SETTINGS_FILE};
use crate::operation_log::{LogType, OperationLogs};
use crate::settings::Settings;
use std::path::Path;
use tokio::sync::mpsc;

use crate::app::tasks;

/// 全局管理器：领域管理器组合根（窗口由 `WindowsManager` 单独持有）
#[derive(Debug)]
pub struct Manager {
    pub logs: OperationLogManager,
    pub config: ConfigManager,
    pub mihomo: MihomoManager,
    pub tasks: TaskChannel,
    pub current_window: WindowId,
    pub should_quit: bool,
}

impl Manager {
    /// 创建管理器与窗口管理器（窗口与 Manager 解绑，需分别持有）
    pub fn new() -> (Self, ui::WindowsManager) {
        let config_dir = dirs::config_dir()
            .expect("无法获取配置目录")
            .join(CONFIG_DIR_NAME);
        if !config_dir.exists() {
            let _ = std::fs::create_dir_all(&config_dir);
        }
        let settings_path = config_dir.join(SETTINGS_FILE);
        let config_path = config_dir.join(CONFIG_FILE);

        let settings = Settings::load_or_create(&settings_path);
        let (tx, rx) = mpsc::channel::<tasks::AsyncTask>(settings.channel_capacity);

        let config = MihomoConfig::read_from_file(&config_path).unwrap_or_else(|_| {
            let c = MihomoConfig::default_config(&settings);
            let _ = c.write_to_path(&config_path);
            c
        });

        let windows = ui::WindowsManager::new(&WindowCtx {
            config: &config,
            config_dir: &config_dir,
        });
        let tun_enabled = config.tun.as_ref().map_or(false, |t| t.enable);
        let status = mihomo::detect_status(&settings, &config_dir);

        let manager = Self {
            logs: OperationLogManager {
                items: OperationLogs::new(),
            },
            config: ConfigManager {
                config,
                config_path,
                settings,
            },
            mihomo: MihomoManager {
                status,
                proxy_running: get_proxy_status().is_ok_and(|(v, _)| v == 1),
                tun_enabled,
                active_node: None,
                is_test_delay: false,
            },
            tasks: TaskChannel { tx, rx },
            current_window: crate::app::ui::pages::main::MAIN,
            should_quit: false,
        };
        (manager, windows)
    }

    fn config_dir(&self) -> &Path {
        self.config
            .config_path
            .parent()
            .unwrap_or(Path::new("."))
    }

    pub fn start_mihomo(&mut self) {
        match mihomo::start_mihomo(
            &self.config.settings,
            &self.config.config_path,
            self.mihomo.tun_enabled,
        ) {
            Ok((pid, binary)) => {
                self.mihomo.status = MihomoStatus::RunningByUs(pid);
                self.logs.add(
                    LogType::Info,
                    format!(
                        "mihomo 已启动 (PID {pid}, {}: {})",
                        binary.source.label(),
                        binary.cmd
                    ),
                );
                tasks::start_and_wait(self.tasks.tx.clone(), self.config.settings.clone());
            }
            Err(e) => self.logs.add(LogType::Error, e),
        }
    }

    pub fn stop_mihomo(&mut self) {
        let config_dir = self.config_dir().to_path_buf();
        match mihomo::stop_mihomo(&self.config.settings, &config_dir) {
            Ok(_) => {
                self.mihomo.status = MihomoStatus::Stopped;
                self.logs.add(LogType::Info, "已停止mihomo".to_string());
            }
            Err(e) => self.logs.add(LogType::Error, e),
        }
    }

    pub fn toggle_mihomo(&mut self) {
        match mihomo::detect_status(&self.config.settings, self.config_dir()) {
            MihomoStatus::Stopped => self.start_mihomo(),
            _ => self.stop_mihomo(),
        }
    }

    pub fn toggle_system_proxy(&mut self) {
        let is_enabled = get_proxy_status()
            .map(|(code, _)| code == 1)
            .unwrap_or(false);
        self.mihomo.proxy_running = !is_enabled;
        if is_enabled {
            match disable_proxy() {
                Ok(_) => self.logs.add(LogType::Info, "关闭系统代理".to_string()),
                Err(e) => self.logs.add(LogType::Error, e.to_string()),
            };
        } else {
            match enable_proxy(&format!(
                "127.0.0.1:{}",
                self.config.settings.mixed_port
            )) {
                Ok(_) => self.logs.add(LogType::Info, "开启系统代理".to_string()),
                Err(e) => self.logs.add(LogType::Error, e.to_string()),
            }
        }
    }

    pub fn toggle_tun(&mut self) {
        let new_state = !self.mihomo.tun_enabled;
        match self
            .config
            .config
            .set_tun_enabled(new_state, &self.config.config_path)
        {
            Ok(()) => {
                self.mihomo.tun_enabled = new_state;
                self.logs.add(
                    LogType::Info,
                    format!("TUN已{}", if new_state { "开启" } else { "关闭" }),
                );
                #[cfg(unix)]
                if new_state {
                    if let Some(warn) = mihomo::tun_capability_warning() {
                        self.logs.add(LogType::Warn, warn);
                    }
                }
                tasks::reload(
                    self.tasks.tx.clone(),
                    self.config.settings.clone(),
                    self.config.config_path.clone(),
                );
            }
            Err(e) => self.logs.add(LogType::Error, e.to_string()),
        }
    }

    /// 异步任务结果回灌：每帧轮询执行
    pub fn poll(&mut self, windows: &mut ui::WindowsManager) {
        while let Ok(task) = self.tasks.rx.try_recv() {
            task(self, windows);
        }
    }

    pub fn load_nodes(&self) {
        let tx = self.tasks.tx.clone();
        tasks::reflash_nodes(tx, self.config.settings.clone());
    }

    pub fn switch_provider(&mut self, name: String) {
        match self
            .config
            .config
            .prepare_switch_provider(&name, &self.config.config_path)
        {
            Ok(()) => {
                self.logs
                    .add(LogType::Info, "正在切换代理商...".to_string());
                tasks::reload(
                    self.tasks.tx.clone(),
                    self.config.settings.clone(),
                    self.config.config_path.clone(),
                );
            }
            Err(e) => self.logs.add(LogType::Error, e.to_string()),
        }
    }
}
