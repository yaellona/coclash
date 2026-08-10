//! 窗口注册表（唯一注册点）。
//!
//! 新增一个页面/弹窗：
//! 1. 在 `ui/windows/` 下新建 `.rs`，窗口类型名以 `Window` 结尾；
//! 2. 在 impl 上加 `#[window]`（弹窗写 `#[window(popup over <Page>)]`），
//!    按键处理器加 `#[key(...)]`，并提供 `new`/`on_open`/`draw`；
//! 3. 在本文件登记一行类型名即可。
pub mod help;
pub mod main;
pub mod mihomo_log;
pub mod provider_select;
pub mod settings;
pub mod url_input;

crate::windows! {
    MainWindow,
    MihomoLogWindow,
    HelpWindow,
    SettingsWindow,
    UrlInputWindow,
    ProviderSelectWindow,
}
