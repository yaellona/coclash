pub mod components;

use crate::app::Manager;
use crate::app::WindowId;
use crate::config::mihomo_config::MihomoConfig;
use crossterm::event::KeyCode;
use ratatui::Frame;
use std::path::Path;
use std::sync::LazyLock;
use ui_derive::windows;

pub mod pages;

#[windows(
    base = "main",
    pages = (mihomo_log),
    popups = (url_input, provider_select, help, settings)
)]
struct _WindowsList;

/// 窗口基类：主窗口与弹窗共有的按键处理与绘制
pub trait Window {
    fn handle_key(&mut self, m: &mut Manager, key: KeyCode);
    fn draw(&mut self, m: &mut Manager, f: &mut Frame);
}

/// 弹窗基类：在 `Window` 之上增加打开钩子
pub trait Popup: Window {
    fn on_open(&mut self, _m: &mut Manager) {}
}

/// 窗口构造上下文：`WindowsManager::new` 与各窗口 `new(ctx)` 的统一参数
pub struct WindowCtx<'a> {
    pub config: &'a MihomoConfig,
    pub config_dir: &'a Path,
}

/// 组件登记项：由 `#[component]` 宏生成
#[derive(Clone, Copy)]
pub struct ComponentEntry {
    pub name: &'static str,
    pub focusable: bool,
}

/// 全部组件的聚合表：新增组件只需在 components/ 下建文件 + 在此加一行
pub static COMPONENTS: LazyLock<Vec<ComponentEntry>> = LazyLock::new(|| {
    let components = vec![
        components::content::COMPONENT_CONTENT,
        components::operation_log::COMPONENT_OPERATION_LOG,
        components::running_info::COMPONENT_RUNNING_INFO,
        components::footer::COMPONENT_FOOTER,
    ];
    let mut names = std::collections::HashSet::new();
    for c in &components {
        assert!(names.insert(c.name), "组件名重复: {}", c.name);
    }
    components
});

/// 可聚焦组件名列表（注册顺序即 Tab 循环顺序）
pub fn focusable_names() -> Vec<&'static str> {
    COMPONENTS
        .iter()
        .filter(|c| c.focusable)
        .map(|c| c.name)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_focusable_components() {
        assert_eq!(focusable_names(), vec!["content", "operation_log"]);
    }

    #[test]
    fn test_component_names_unique() {
        let mut names = std::collections::HashSet::new();
        for c in COMPONENTS.iter() {
            assert!(names.insert(c.name), "重复组件名: {}", c.name);
        }
        assert_eq!(COMPONENTS.len(), 4);
    }
}
