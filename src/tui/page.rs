//! 页面统一接口：所有页面/弹窗的公共契约。
//!
//! 页面实现者只需：
//! - 在 `impl Page` 里声明 `const BINDINGS`（快捷键 + 执行函数 + 描述）与 `draw`；
//! - 用固有方法写按键处理器（签名 `fn(&mut self, state: &AppState) -> Cmd`，
//!   需要原始按键时用 `Binding::text` 的 `fn(&mut self, state, key)`）；
//! - 弹窗页覆写 `parent()`；打开时需要重置状态的可覆写 `on_open()`。
//!
//! 页面**只读** `AppState`，不持有 Manager：所有副作用表达为 `Cmd`，
//! 由注册表（`Pages::dispatch`）统一执行。这样页面逻辑可脱离 IO 单测。
//!
//! `PageOps` 是对象安全适配器（`Page` 带关联常量无法做成 trait object），
//! 让注册表用 `Box<dyn PageOps>` 统一持有全部页面。
use crate::manager::state::AppState;
use crate::tui::action::{Binding, dispatch};
use crate::tui::cmd::Cmd;
use crate::tui::pages::PageId;
use crossterm::event::KeyEvent;
use ratatui::Frame;

pub trait Page: Sized + 'static {
    /// 本页全部按键绑定（快捷键 / 执行函数 / 描述集中声明在这里）
    const BINDINGS: &'static [Binding<Self>];

    /// 弹窗父页面；`None` = 全屏页面
    fn parent(&self) -> Option<PageId> {
        None
    }

    /// 打开页面时触发（默认无操作）
    fn on_open(&mut self) {}

    /// 绘制（弹窗由注册表先画父页面再叠加）；只读渲染，修改请返回 `Cmd`
    fn draw(&mut self, state: &AppState, f: &mut Frame);

    /// 按键分发：返回 `Cmd`（导航 + 副作用），由注册表执行
    fn handle_key(&mut self, state: &AppState, key: KeyEvent) -> Cmd {
        dispatch(Self::BINDINGS, self, state, key)
    }
}

/// 对象安全适配器：注册表通过它统一驱动所有页面
pub trait PageOps {
    fn parent(&self) -> Option<PageId>;
    fn on_open(&mut self);
    fn draw(&mut self, state: &AppState, f: &mut Frame);
    fn handle_key(&mut self, state: &AppState, key: KeyEvent) -> Cmd;
}

impl<T: Page> PageOps for T {
    fn parent(&self) -> Option<PageId> {
        Page::parent(self)
    }

    fn on_open(&mut self) {
        Page::on_open(self)
    }

    fn draw(&mut self, state: &AppState, f: &mut Frame) {
        Page::draw(self, state, f)
    }

    fn handle_key(&mut self, state: &AppState, key: KeyEvent) -> Cmd {
        Page::handle_key(self, state, key)
    }
}
