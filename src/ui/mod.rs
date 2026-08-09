pub mod keymap;
pub mod layout;
pub mod scroll;
pub mod widgets;
pub mod windows;

pub use windows::Windows;

/// 窗口身份：主窗口/全屏页面/弹窗（替代原字符串 WindowId）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Page {
    Main,
    MihomoLog,
    Help,
    Settings,
    UrlInput,
    ProviderSelect,
}
