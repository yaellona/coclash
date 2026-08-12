//! 管理器层：持有共享数据（`Arc<Mutex<AppState>>`）与任务层（TaskRunner），提供命令面；不依赖 tui。
//!
//! # 线程模型
//!
//! - `AppState` 是唯一共享数据，后台任务直接读写（详见 `task` 模块的并发纪律）
//! - TUI 层只读：通过 `state_lock()` 短暂持锁读取；修改一律走 Manager 命令或
//!   `AppState` 方法（禁止在窗口里直接字段赋值，如 `st.select = ...`）
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
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, MutexGuard};

use state::AppState;
use task::TaskRunner;

/// 管理器：共享数据与任务的中枢，TUI 只通过它发命令与读状态。
pub struct Manager {
    /// 唯一共享数据（多线程可读可写）
    pub state: Arc<Mutex<AppState>>,
    pub settings: Settings,
    pub config_path: PathBuf,
    pub should_quit: AtomicBool,
    tasks: TaskRunner,
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

        let config = match MihomoConfig::read_from_file(&config_path) {
            Ok(config) => config,
            Err(e) => {
                if config_path.exists() {
                    // 解析失败：保留并备份原文件，内存中退回默认配置，绝不静默覆盖用户数据
                    eprintln!(
                        "config.yaml 解析失败({e})，本次使用默认配置；原文件已保留并备份为 config.yaml.bak"
                    );
                    let _ = std::fs::copy(&config_path, config_path.with_extension("yaml.bak"));
                    MihomoConfig::default_config()
                } else {
                    // 首次运行：生成默认配置
                    let config = MihomoConfig::default_config();
                    if let Err(e) = config.write_to_path(&config_path) {
                        eprintln!("写入默认 config.yaml 失败: {e}");
                    }
                    config
                }
            }
        };
        let group_name = config.group_name().to_string();

        let state = Arc::new(Mutex::new(AppState {
            nodes: vec![],
            select: 0,
            active_node: None,
            mihomo_status: MihomoStatus::Stopped,
            proxy_running: false,
            is_test_delay: false,
            is_switching_node: false,
            logs: OperationLogs::new(),
            config,
        }));
        {
            let mut st = state.lock().unwrap_or_else(|e| e.into_inner());
            st.mihomo_status = mihomo::detect_status(&settings, &config_dir);
            st.proxy_running = get_proxy_status().is_ok_and(|(v, _)| v == 1);
        }

        let tasks = TaskRunner::new(&settings, config_path.clone(), &group_name, state.clone())?;

        Ok(Self {
            state,
            settings,
            config_path,
            should_quit: AtomicBool::new(false),
            tasks,
        })
    }

    pub fn config_dir(&self) -> &Path {
        self.config_path.parent().unwrap_or(Path::new("."))
    }

    /// 毒锁恢复的锁获取（全项目统一入口）
    pub fn state_lock(&self) -> MutexGuard<'_, AppState> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    // ===== 日志 =====

    pub fn log(&self, msg: impl Into<String>) {
        self.state_lock().logs.add_log(LogType::Info, msg.into());
    }

    pub fn log_err(&self, e: impl std::fmt::Display) {
        self.state_lock()
            .logs
            .add_log(LogType::Error, e.to_string());
    }

    pub fn log_warn(&self, msg: impl Into<String>) {
        self.state_lock().logs.add_log(LogType::Warn, msg.into());
    }

    // ===== 配置编辑（锁内纯内存修改，落盘走 save_config） =====

    /// 配置编辑（锁内纯内存修改，落盘走 save_config）；返回闭包结果
    pub fn edit_config<R>(&self, f: impl FnOnce(&mut MihomoConfig) -> R) -> R {
        let mut st = self.state_lock();
        f(&mut st.config)
    }

    /// 配置落盘（序列化在锁内、写盘在锁外），成功后由调用方决定是否重载
    pub fn save_config(&self) -> Result<(), Error> {
        self.tasks.save_config()
    }

    // ===== 命令（同步，按键直接触发） =====

    pub fn start_mihomo(&self) {
        let elevate = self.state_lock().tun_enabled();
        match mihomo::start_mihomo(&self.settings, &self.config_path, elevate) {
            Ok((pid, binary)) => {
                self.state_lock().mihomo_status = MihomoStatus::RunningByUs(pid);
                self.log(format!(
                    "mihomo 已启动 (PID {pid}, {}: {})",
                    binary.source.label(),
                    binary.cmd
                ));
                self.tasks.wait_for_start();
            }
            Err(e) => self.log_err(e),
        }
    }

    pub fn stop_mihomo(&self) {
        let config_dir = self.config_dir().to_path_buf();
        match mihomo::stop_mihomo(&self.settings, &config_dir) {
            Ok(()) => {
                self.state_lock().mihomo_status = MihomoStatus::Stopped;
                self.log("已停止mihomo");
            }
            Err(e) => self.log_err(e),
        }
    }

    pub fn toggle_mihomo(&self) {
        let config_dir = self.config_dir().to_path_buf();
        match mihomo::detect_status(&self.settings, &config_dir) {
            MihomoStatus::Stopped => self.start_mihomo(),
            _ => self.stop_mihomo(),
        }
    }

    pub fn toggle_system_proxy(&self) {
        let is_enabled = get_proxy_status()
            .map(|(code, _)| code == 1)
            .unwrap_or(false);
        self.state_lock().proxy_running = !is_enabled;
        if is_enabled {
            match disable_proxy() {
                Ok(()) => self.log("关闭系统代理"),
                Err(e) => self.log_err(e),
            }
        } else {
            let addr = self.state_lock().proxy_addr();
            match enable_proxy(&addr) {
                Ok(()) => self.log("开启系统代理"),
                Err(e) => self.log_err(e),
            }
        }
    }

    pub fn toggle_tun(&self) {
        let new_state = !self.state_lock().tun_enabled();
        self.edit_config(|c| c.set_tun_enabled(new_state));
        self.log(format!("TUN已{}", if new_state { "开启" } else { "关闭" }));
        #[cfg(unix)]
        if new_state && let Some(warn) = mihomo::tun_capability_warning() {
            self.log_warn(warn);
        }
        if let Err(e) = self.save_config() {
            self.log_err(e);
            return;
        }
        self.reload_config();
    }

    pub fn switch_provider(&self, name: String) {
        let result = {
            let mut st = self.state_lock();
            st.config.prepare_switch_provider(&name)
        };
        match result {
            Ok(()) => {
                self.log("正在切换订阅...");
                if let Err(e) = self.save_config() {
                    self.log_err(e);
                    return;
                }
                self.reload_config();
            }
            Err(e) => self.log_err(e),
        }
    }

    // ===== 任务发起（一行转发，任务逻辑在 manager/task.rs） =====

    pub fn switch_node(&self, index: usize) {
        self.tasks.switch_node(index);
    }

    pub fn start_delay_test(&self) {
        self.tasks.start_delay_test();
    }

    pub fn load_nodes(&self) {
        self.tasks.load_nodes();
    }

    pub fn reload_config(&self) {
        self.tasks.reload_config();
    }

    pub fn insert_sub(&self, url: String) {
        self.tasks.insert_sub(url);
    }
}
