//! coclash 应用库：分层结构。
//!
//! - `core`：mihomo 基本操作（API 调用、进程管理、配置读写），不依赖上层
//! - `manager`：数据（AppState）与任务层（TaskBus），只依赖 core
//! - `tui`：绘制与按键，只依赖 manager
//! - `settings`/`constants`/`error`/`operation_log`：共享叶子
//!
//! `src/main.rs` 是薄入口（终端初始化 + 事件循环）。

pub mod core;
pub mod manager;
pub mod tui;

pub mod constants;
pub mod error;
pub mod operation_log;
pub mod settings;
#[cfg(test)]
mod test;

/// 过程宏 re-export：`#[window]`（窗口 impl 标注）+ `windows!`（窗口注册表）。
pub use coclash_macros::{window, windows};
