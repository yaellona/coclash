//! UI 动作词汇表：页面 handler 只返回 `Cmd`，不直接触碰 Manager。
//!
//! - `Nav`：纯 UI 导航（打开页面 / 返回父页 / 退出），由 `Pages` 注册表处理；
//! - `Effect`：副作用（manager::commands），由注册表转发给 `Manager::exec`。
//!
//! 两者可同时发生（例如「插入订阅并返回主界面」）。
use crate::manager::commands::Effect;
use crate::tui::pages::PageId;

/// 导航动作（UI 级）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Nav {
    None,
    Goto(PageId),
    /// 返回父页（按 `Page::parent` 解析，弹窗不硬编码目标页）
    Back,
    Quit,
}

/// 页面动作 = 导航 + 副作用
#[derive(Debug, Clone, PartialEq)]
pub struct Cmd {
    pub nav: Nav,
    pub effect: Effect,
}

impl Cmd {
    pub const fn none() -> Self {
        Self {
            nav: Nav::None,
            effect: Effect::None,
        }
    }

    pub const fn goto(page: PageId) -> Self {
        Self {
            nav: Nav::Goto(page),
            effect: Effect::None,
        }
    }

    pub const fn back() -> Self {
        Self {
            nav: Nav::Back,
            effect: Effect::None,
        }
    }

    pub const fn quit() -> Self {
        Self {
            nav: Nav::Quit,
            effect: Effect::None,
        }
    }

    pub const fn effect(effect: Effect) -> Self {
        Self {
            nav: Nav::None,
            effect,
        }
    }

    pub const fn new(nav: Nav, effect: Effect) -> Self {
        Self { nav, effect }
    }
}
