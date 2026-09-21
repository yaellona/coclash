//! 命令层：UI 可请求的全部副作用（`Effect`）与本地配置修改（`ConfigChange`）。
//!
//! UI 的按键处理器不直接触碰 Manager，而是把动作表达成数据（tui 层的 `Cmd`），
//! 由注册表统一调用 `Manager::exec` 执行；因此 `Effect`/`ConfigChange` 是
//! 层间契约，也是单测的断言对象。
use super::Manager;

/// UI 可请求的副作用（数据，不是闭包）。
#[derive(Debug, Clone, PartialEq)]
pub enum Effect {
    None,
    SyncNow,
    ToggleMihomo,
    ToggleSystemProxy,
    ToggleTun,
    DelayTest,
    SwitchNode(usize),
    SwitchProvider(String),
    DeleteProvider(String),
    InsertSub(String),
    Config(ConfigChange),
    SaveAndReload,
}

/// 本地配置修改（设置页可产生的全部编辑动作）。
#[derive(Debug, Clone, PartialEq)]
pub enum ConfigChange {
    Mode(String),
    LogLevel(String),
    AllowLan(bool),
    UnifiedDelay(bool),
    Tun(bool),
    Dns(bool),
    Port(u16),
    SocksPort(u16),
    KeepAlive(u32),
    SetRule { index: usize, text: String },
    AddRule(String),
    RemoveRule(usize),
}

impl Manager {
    /// 执行一个副作用（UI 的唯一副作用入口）。
    pub fn exec(&self, effect: Effect) {
        match effect {
            Effect::None => {}
            Effect::SyncNow => self.sync_now(),
            Effect::ToggleMihomo => self.toggle_mihomo(),
            Effect::ToggleSystemProxy => self.toggle_system_proxy(),
            Effect::ToggleTun => self.toggle_tun(),
            Effect::DelayTest => self.start_delay_test(),
            Effect::SwitchNode(index) => self.switch_node(index),
            Effect::SwitchProvider(name) => self.switch_provider(name),
            Effect::DeleteProvider(name) => self.delete_provider(name),
            Effect::InsertSub(url) => self.insert_sub(url),
            Effect::Config(change) => self.apply_config(change),
            Effect::SaveAndReload => self.save_and_reload(),
        }
    }

    /// 应用一条配置修改（仅内存；落盘由 `SaveAndReload`/`toggle_tun` 等负责）。
    /// TUN/DNS 与快捷键路径共用同一份逻辑，避免两处实现分叉。
    pub fn apply_config(&self, change: ConfigChange) {
        match change {
            ConfigChange::Mode(v) => self.edit_config(|c| c.mode = v),
            ConfigChange::LogLevel(v) => self.edit_config(|c| c.log_level = v),
            ConfigChange::AllowLan(on) => self.edit_config(|c| c.allow_lan = on),
            ConfigChange::UnifiedDelay(on) => self.edit_config(|c| c.unified_delay = on),
            ConfigChange::Tun(on) => {
                self.edit_config(|c| c.set_tun_enabled(on));
                #[cfg(unix)]
                if on && let Some(warn) = crate::core::mihomo::tun_capability_warning() {
                    self.log_warn(warn);
                }
            }
            ConfigChange::Dns(on) => self.edit_config(|c| c.set_dns_enabled(on)),
            ConfigChange::Port(v) => self.edit_config(|c| c.port = v),
            ConfigChange::SocksPort(v) => self.edit_config(|c| c.socks_port = v),
            ConfigChange::KeepAlive(v) => self.edit_config(|c| c.keep_alive_interval = v),
            ConfigChange::SetRule { index, text } => self.edit_config(|c| {
                if index < c.rules.len() {
                    c.rules[index] = text;
                }
            }),
            ConfigChange::AddRule(text) => self.edit_config(|c| c.rules.push(text)),
            ConfigChange::RemoveRule(index) => self.edit_config(|c| {
                if index < c.rules.len() {
                    c.rules.remove(index);
                }
            }),
        }
    }

    /// 删除订阅：内存移除 + 落盘 + 重载 mihomo
    pub fn delete_provider(&self, name: String) {
        self.edit_config(|c| c.remove_provider(&name));
        self.save_and_reload();
    }

    /// 落盘 + 重载（失败只记日志）
    pub fn save_and_reload(&self) {
        match self.save_config() {
            Ok(()) => self.reload_config(),
            Err(e) => self.log_err(e),
        }
    }
}
