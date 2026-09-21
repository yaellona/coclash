//! coclash 应用库：分层结构。
//!
//! - `core`：mihomo 基本操作（API 调用、进程管理、配置读写），不依赖上层
//! - `manager`：共享数据（`AppState`）与命令层，只依赖 core
//!   - `state.rs`：`AppState { logs, config, mihomo }`
//!   - `commands.rs`：UI 副作用契约（`Effect`/`ConfigChange`）与 `Manager::exec`
//!   - `tasks.rs`：用户触发的异步任务；`heartbeat.rs`：唯一的状态同步实现
//! - `tui`：绘制与按键，只依赖 manager
//!   - `cmd.rs`：页面动作（`Nav` + `Effect`）
//!   - `action.rs`：按键绑定（快捷键 + 执行函数 + 描述）
//!   - `page.rs`：`Page` trait（只读 `AppState`，返回 `Cmd`）
//!   - `pages/`：各页面实现 + 手写注册表（无过程宏）
//! - `settings`/`constants`/`error`/`operation_log`：共享叶子
//!
//! `src/main.rs` 是薄入口（终端初始化 + 事件循环 + 心跳启动）。

pub mod core;
pub mod manager;
pub mod tui;

pub mod cli;
pub mod constants;
pub mod error;
pub mod operation_log;
pub mod settings;
#[cfg(test)]
mod test;
