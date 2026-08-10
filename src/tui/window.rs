//! 窗口统一接口：所有页面/弹窗的公共契约。
//!
//! 用户窗口以固有方法书写 `new`/`on_open`/`draw` + `#[key]` 处理器，
//! `#[window]` 宏负责生成 `impl Window`（校验契约、生成 `handle_key` 与 `meta`）。
use crate::manager::Manager;
use crate::tui::Page;
use crossterm::event::KeyEvent;
use ratatui::Frame;

/// 窗口页面元数据
#[derive(Clone, Copy)]
pub struct WindowMeta {
    /// 弹窗父页面；`None` = 全屏页面
    pub parent: Option<Page>,
}

/// 页面/弹窗统一接口（全部方法均可 dyn 分发，`new` 为固有方法约定）。
pub trait Window {
    /// 页面元数据：由 `#[window]` 宏生成
    fn meta(&self) -> WindowMeta;

    /// 打开页面时触发
    fn on_open(&mut self);

    /// 绘制（弹窗由注册表先画父页面再叠加）
    fn draw(&mut self, manager: &mut Manager, f: &mut Frame);

    /// 按键分发：返回 `Some(page)` 表示请求导航；由 `#[window]` 宏生成
    fn handle_key(&mut self, manager: &mut Manager, key: KeyEvent) -> Option<Page>;
}
