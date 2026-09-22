//! 管理器层：持有共享数据与全部命令面；不依赖 tui。
//!
//! # 结构
//!
//! ```text
//! Manager ──┬── state.rs    AppState（logs/config/mihomo 三块）
//!           ├── commands.rs UI 副作用契约（Effect/ConfigChange）与统一入口 exec
//!           ├── tasks.rs    用户触发的异步任务（impl Manager）
//!           └── heartbeat.rs 心跳：定时同步 mihomo 运行时信息（impl Manager）
//! ```
//!
//! # 线程模型
//!
//! - `Shared` 是唯一共享数据（`Arc<Shared>`），TUI 层只读（`state_lock`），
//!   修改一律走 Manager 命令；后台任务直接读写 `Shared`（见 tasks/heartbeat 的并发纪律）。
//! - 心跳是唯一的状态同步实现：外部启停 mihomo、外部切节点/改配置都会被心跳感知。
pub mod commands;
pub mod heartbeat;
pub mod state;
pub mod tasks;

use crate::constants::{CONFIG_DIR_NAME, CONFIG_FILE, SETTINGS_FILE};
use crate::core::config::mihomo_config::MihomoConfig;
use crate::core::mihomo::{self, ApiClient, MihomoStatus};
use crate::core::system_proxy::{disable_proxy, enable_proxy, get_proxy_status};
use crate::error::Error;
use crate::operation_log::{LogType, OperationLogs};
use crate::settings::Settings;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use tokio::sync::Notify;

use state::{AppState, MihomoState};

/// 共享数据：Manager 与后台任务（tasks/heartbeat）共享的单一结构。
pub(crate) struct Shared {
    pub state: Mutex<AppState>,
    pub settings: Settings,
    pub config_path: PathBuf,
    pub api: ApiClient,
    /// 配置写盘串行化：防止两个保存交错导致旧快照覆盖新快照
    pub save_lock: Mutex<()>,
    /// 重绘请求标志：状态变更置位，主循环消费后清零（dirty-flag 条件重绘）
    pub redraw: AtomicBool,
    pub should_quit: AtomicBool,
    /// 立即同步请求：`sync_now` 唤醒一次心跳并强制全量同步
    pub sync_notify: Notify,
    pub force_full: AtomicBool,
}

impl Shared {
    /// 毒锁恢复的锁获取（后台任务统一入口）
    pub fn lock(&self) -> MutexGuard<'_, AppState> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn mark_redraw(&self) {
        self.redraw.store(true, Ordering::Relaxed);
    }

    /// 请求立即同步一次（唤醒心跳并强制全量）
    pub fn sync_now(&self) {
        self.force_full.store(true, Ordering::Relaxed);
        self.sync_notify.notify_one();
    }

    /// 后台任务日志：写操作记录并请求重绘
    pub fn log(&self, log_type: LogType, msg: impl Into<String>) {
        self.lock().logs.add_log(log_type, msg.into());
        self.mark_redraw();
    }
}

/// 管理器：共享数据与命令的中枢，TUI 只通过它发命令与读状态。
pub struct Manager {
    shared: Arc<Shared>,
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
        let api = ApiClient::new(&settings, config.group_name())?;

        let state = Mutex::new(AppState {
            logs: OperationLogs::new(),
            config,
            mihomo: MihomoState::default(),
        });
        {
            let mut st = state.lock().unwrap_or_else(|e| e.into_inner());
            st.mihomo.status = mihomo::detect_status(&settings);
            st.mihomo.proxy_running = get_proxy_status().is_ok_and(|(v, _)| v == 1);
        }

        Ok(Self {
            shared: Arc::new(Shared {
                state,
                settings,
                config_path,
                api,
                save_lock: Mutex::new(()),
                redraw: AtomicBool::new(true),
                should_quit: AtomicBool::new(false),
                sync_notify: Notify::new(),
                force_full: AtomicBool::new(false),
            }),
        })
    }

    /// 后台任务捕获用（tasks/heartbeat 在同一 crate 内）
    pub(crate) fn shared(&self) -> &Arc<Shared> {
        &self.shared
    }

    pub fn settings(&self) -> &Settings {
        &self.shared.settings
    }

    pub fn config_dir(&self) -> &Path {
        self.shared.config_path.parent().unwrap_or(Path::new("."))
    }

    /// 毒锁恢复的锁获取（全项目统一入口）
    pub fn state_lock(&self) -> MutexGuard<'_, AppState> {
        self.shared.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// 标记需要重绘（状态变更的统一信号，主循环消费后清零）
    pub fn mark_redraw(&self) {
        self.shared.redraw.store(true, Ordering::Relaxed);
    }

    /// 主循环消费重绘标志
    pub fn take_redraw(&self) -> bool {
        self.shared.redraw.swap(false, Ordering::Relaxed)
    }

    pub fn request_quit(&self) {
        self.shared.should_quit.store(true, Ordering::Relaxed);
    }

    pub fn quit_requested(&self) -> bool {
        self.shared.should_quit.load(Ordering::Relaxed)
    }

    // ===== 日志 =====

    pub fn log(&self, msg: impl Into<String>) {
        self.state_lock().logs.add_log(LogType::Info, msg.into());
        self.mark_redraw();
    }

    pub fn log_err(&self, e: impl std::fmt::Display) {
        self.state_lock()
            .logs
            .add_log(LogType::Error, e.to_string());
        self.mark_redraw();
    }

    pub fn log_warn(&self, msg: impl Into<String>) {
        self.state_lock().logs.add_log(LogType::Warn, msg.into());
        self.mark_redraw();
    }

    // ===== 配置编辑（锁内纯内存修改，落盘走 save_config） =====

    /// 配置编辑（锁内纯内存修改，落盘走 save_config）；返回闭包结果
    pub fn edit_config<R>(&self, f: impl FnOnce(&mut MihomoConfig) -> R) -> R {
        let mut st = self.state_lock();
        let r = f(&mut st.config);
        drop(st);
        self.mark_redraw();
        r
    }

    /// 配置落盘（锁内克隆、锁外序列化 + 写盘），成功后由调用方决定是否重载
    pub fn save_config(&self) -> Result<(), Error> {
        tasks::write_config(&self.shared)
    }

    // ===== 命令（同步，按键直接触发） =====

    pub fn start_mihomo(&self) {
        // Windows 上只要启动 mihomo 就请求管理员权限（TUN 或普通代理都提权）
        let elevate = cfg!(windows);
        match mihomo::start_mihomo(&self.shared.settings, &self.shared.config_path, elevate) {
            Ok((pid, source)) => {
                self.state_lock().mihomo.status = MihomoStatus::Running(pid);
                self.log(format!(
                    "mihomo 已启动 (PID {pid}, 内嵌 {}: {source})",
                    mihomo::embedded::VERSION,
                ));
                // 端口绑定需要时间：延迟触发同步（心跳开启=唤醒，关闭=一次性任务）
                heartbeat::sync_now_delayed(&self.shared, std::time::Duration::from_millis(800));
                self.mark_redraw();
            }
            Err(e) => self.log_err(e),
        }
    }

    pub fn stop_mihomo(&self) {
        match mihomo::stop_mihomo(self.settings()) {
            Ok(()) => {
                self.state_lock().mihomo.status = MihomoStatus::Stopped;
                self.log("已停止mihomo");
                self.mark_redraw();
            }
            Err(e) => self.log_err(e),
        }
    }

    /// 直接按端口启停：端口可达 = 运行中 → 停止；否则启动（不判断归属）。
    /// 启停含 UAC 等待、信号轮询等阻塞 IO，交给异步任务（tasks.rs），不卡 UI。
    pub fn toggle_mihomo(&self) {
        tasks::toggle_mihomo(&self.shared);
    }

    pub fn toggle_system_proxy(&self) {
        let is_enabled = get_proxy_status()
            .map(|(code, _)| code == 1)
            .unwrap_or(false);
        if is_enabled {
            match disable_proxy() {
                Ok(()) => {
                    self.state_lock().mihomo.proxy_running = false;
                    self.log("关闭系统代理");
                }
                Err(e) => self.log_err(e),
            }
        } else {
            // 端口未知（未配置/运行时未就绪）时不能拿 0 当端口
            let Some(addr) = self.state_lock().proxy_addr() else {
                self.log_err("未配置代理端口，无法开启系统代理");
                return;
            };
            match enable_proxy(&addr) {
                Ok(()) => {
                    self.state_lock().mihomo.proxy_running = true;
                    self.log(format!("开启系统代理 ({addr})"));
                }
                Err(e) => self.log_err(e),
            }
        }
        self.mark_redraw();
    }

    pub fn toggle_tun(&self) {
        let new_state = !self.state_lock().tun_enabled();
        // 与设置页 TUN 字段共用同一份配置逻辑（commands::ConfigChange::Tun）
        self.apply_config(commands::ConfigChange::Tun(new_state));
        self.log(format!("TUN已{}", if new_state { "开启" } else { "关闭" }));
        self.save_and_reload();
    }

    pub fn switch_provider(&self, name: String) {
        let result = {
            let mut st = self.state_lock();
            st.config.prepare_switch_provider(&name)
        };
        match result {
            Ok(()) => {
                self.log("正在切换订阅...");
                self.save_and_reload();
            }
            Err(e) => self.log_err(e),
        }
    }
}
