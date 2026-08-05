pub mod event;
pub mod keymap;
pub mod managers;
pub mod mihomo_log;
pub mod tasks;
pub mod ui;

pub use managers::Manager;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WindowId(pub &'static str);

/// 可聚焦的面板（类型安全，穷尽匹配）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Panel {
    Content,
    OperationLog,
}

impl Panel {
    pub fn name(&self) -> &'static str {
        match self {
            Panel::Content => "content",
            Panel::OperationLog => "operation_log",
        }
    }

    pub fn from_name(name: &str) -> Option<Panel> {
        match name {
            "content" => Some(Panel::Content),
            "operation_log" => Some(Panel::OperationLog),
            _ => None,
        }
    }

    /// 按组件注册表顺序切换到下一个可聚焦面板
    pub fn cycle(&self) -> Panel {
        let names = crate::app::ui::focusable_names();
        if names.len() < 2 {
            return *self;
        }
        let idx = names
            .iter()
            .position(|n| *n == self.name())
            .unwrap_or(0);
        Panel::from_name(names[(idx + 1) % names.len()]).unwrap_or(*self)
    }
}
