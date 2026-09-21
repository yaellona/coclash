//! 页面注册表（唯一登记点）。
//!
//! 新增一个页面/弹窗：
//! 1. 在 `tui/pages/` 下新建 `.rs`，实现 `Page` trait（`BINDINGS` + `draw`，弹窗加 `parent`）；
//! 2. 在本文件加 `pub mod` + `PageId` 变体 + `Pages::new` 里 `slots.insert(...)`；
//! 3. 在 `BINDINGS` 聚合里加一行（帮助/底栏数据源）。
//!
//! 导航与副作用都在 `Pages::dispatch` 集中处理：页面返回 `Cmd`，
//! `Nav` 由本层执行，`Effect` 转发给 `Manager::exec`。
use crate::constants::MIHOMO_LOG_FILE;
use crate::manager::Manager;
use crate::tui::action::{BindingMeta, binding_meta};
use crate::tui::cmd::{Cmd, Nav};
use crate::tui::page::{Page, PageOps};
use crossterm::event::KeyEvent;
use ratatui::Frame;
use std::collections::HashMap;
use std::sync::LazyLock;

pub mod help;
pub mod main;
pub mod mihomo_log;
pub mod provider_select;
pub mod settings;
pub mod url_input;

pub use help::HelpPage;
pub use main::MainPage;
pub use mihomo_log::MihomoLogPage;
pub use provider_select::ProviderSelectPage;
pub use settings::SettingsPage;
pub use url_input::UrlInputPage;

/// 页面身份（初始页：Main）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PageId {
    Main,
    MihomoLog,
    Help,
    Settings,
    UrlInput,
    ProviderSelect,
}

/// 页面管理器：持有全部页面，负责导航、动作分发与绘制。
pub struct Pages {
    pub current: PageId,
    slots: HashMap<PageId, Box<dyn PageOps>>,
}

impl Pages {
    pub fn new(manager: &Manager) -> Self {
        let mut slots: HashMap<PageId, Box<dyn PageOps>> = HashMap::new();
        slots.insert(PageId::Main, Box::new(MainPage::new()));
        slots.insert(PageId::Help, Box::new(HelpPage::new()));
        slots.insert(PageId::Settings, Box::new(SettingsPage::new()));
        slots.insert(PageId::UrlInput, Box::new(UrlInputPage::new()));
        slots.insert(
            PageId::MihomoLog,
            Box::new(MihomoLogPage::new(
                manager.config_dir().join(MIHOMO_LOG_FILE),
            )),
        );
        {
            let state = manager.state_lock();
            slots.insert(
                PageId::ProviderSelect,
                Box::new(ProviderSelectPage::new(&state)),
            );
        }
        Self {
            current: PageId::Main,
            slots,
        }
    }

    fn slot(&self, page: PageId) -> &dyn PageOps {
        self.slots.get(&page).expect("页面未注册").as_ref()
    }

    fn slot_mut(&mut self, page: PageId) -> &mut dyn PageOps {
        self.slots.get_mut(&page).expect("页面未注册").as_mut()
    }

    /// 导航到某页；页面切换时触发对应页面的 `on_open` 钩子
    pub fn open(&mut self, page: PageId) {
        if page == self.current {
            return;
        }
        self.current = page;
        self.slot_mut(page).on_open();
    }

    /// 返回当前页的父页（弹窗关闭语义）
    fn back(&mut self) {
        if let Some(parent) = self.slot(self.current).parent() {
            self.open(parent);
        }
    }

    /// 按键分发：页面只读状态算出 `Cmd`（锁在算完即释放），再由 dispatch 执行
    pub fn handle_key(&mut self, manager: &Manager, key: KeyEvent) {
        let current = self.current;
        let cmd = {
            let state = manager.state_lock();
            self.slot_mut(current).handle_key(&state, key)
        };
        self.dispatch(manager, cmd);
    }

    /// 绘制当前页
    pub fn draw(&mut self, manager: &Manager, f: &mut Frame) {
        self.draw_page(self.current, manager, f);
    }

    /// 绘制某页：若该页是弹窗（`Page::parent` 声明了父页面），先递归画父页面再叠加自己
    fn draw_page(&mut self, page: PageId, manager: &Manager, f: &mut Frame) {
        if let Some(parent) = self.slot(page).parent() {
            self.draw_page(parent, manager, f);
        }
        let state = manager.state_lock();
        self.slot_mut(page).draw(&state, f);
    }

    /// 执行页面动作：导航在本层处理，副作用转发 Manager
    fn dispatch(&mut self, manager: &Manager, cmd: Cmd) {
        manager.exec(cmd.effect);
        match cmd.nav {
            Nav::None => {}
            Nav::Goto(page) => self.open(page),
            Nav::Back => self.back(),
            Nav::Quit => manager.request_quit(),
        }
    }
}

/// 全部页面按键聚合（帮助弹窗和底部栏据此自动生成）。
pub static BINDINGS: LazyLock<Vec<BindingMeta>> = LazyLock::new(|| {
    let mut bindings = Vec::new();
    bindings.extend(binding_meta(PageId::Main, MainPage::BINDINGS));
    bindings.extend(binding_meta(PageId::MihomoLog, MihomoLogPage::BINDINGS));
    bindings.extend(binding_meta(PageId::Help, HelpPage::BINDINGS));
    bindings.extend(binding_meta(PageId::Settings, SettingsPage::BINDINGS));
    bindings.extend(binding_meta(PageId::UrlInput, UrlInputPage::BINDINGS));
    bindings.extend(binding_meta(
        PageId::ProviderSelect,
        ProviderSelectPage::BINDINGS,
    ));
    bindings
});

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tui::action::{BindingMeta, KeyPattern, footer_text, help_rows, key_label};
    use crossterm::event::KeyCode;

    /// 全部页面（用于逐页校验绑定表）
    fn all_pages() -> [PageId; 6] {
        [
            PageId::Main,
            PageId::MihomoLog,
            PageId::Help,
            PageId::Settings,
            PageId::UrlInput,
            PageId::ProviderSelect,
        ]
    }

    fn bindings_of(page: PageId) -> Vec<BindingMeta> {
        BINDINGS
            .iter()
            .filter(|b| b.page == page)
            .copied()
            .collect()
    }

    #[test]
    fn test_no_duplicate_exact_keys_per_page() {
        for page in all_pages() {
            let mut keys: Vec<String> = bindings_of(page)
                .iter()
                .map(|b| match b.key {
                    KeyPattern::Code(c) => format!("Code:{c:?}"),
                    KeyPattern::AnyChar => "AnyChar".to_string(),
                })
                .collect();
            keys.sort();
            let deduped = keys.clone();
            keys.dedup();
            assert_eq!(keys, deduped, "页面 {page:?} 存在重复精确按键");
        }
    }

    #[test]
    fn test_registry_has_main_bindings() {
        let main: Vec<BindingMeta> = bindings_of(PageId::Main);
        let has = |k: KeyCode| {
            main.iter()
                .any(|b| matches!(b.key, KeyPattern::Code(c) if c == k))
        };
        assert!(has(KeyCode::Char('q')));
        assert!(has(KeyCode::Up));
        assert!(has(KeyCode::Tab));
        assert!(has(KeyCode::Esc));
        assert!(has(KeyCode::PageDown));
        assert!(
            bindings_of(PageId::UrlInput)
                .iter()
                .any(|b| matches!(b.key, KeyPattern::Code(KeyCode::Enter)))
        );
        assert!(
            bindings_of(PageId::MihomoLog)
                .iter()
                .any(|b| matches!(b.key, KeyPattern::Code(KeyCode::PageDown)))
        );
        assert!(
            bindings_of(PageId::Help)
                .iter()
                .any(|b| matches!(b.key, KeyPattern::Code(KeyCode::Up)))
        );
    }

    #[test]
    fn test_exact_precedes_wildcard() {
        // 设置页既有精确键（r/a/d/Enter/Esc…）也有 AnyChar：
        // 精确按键必须命中对应处理器而不会落到 AnyChar
        let settings = bindings_of(PageId::Settings);
        let code = |c: KeyCode| settings.iter().find(|b| b.key == KeyPattern::Code(c));
        assert!(code(KeyCode::Enter).is_some());
        assert!(code(KeyCode::Esc).is_some());
    }

    #[test]
    fn test_footer_and_help_generated() {
        let footer = footer_text();
        assert!(footer.contains("q: 退出"));
        assert!(footer.contains("↑/↓: 导航"));
        assert!(footer.contains("?: 帮助"));
        assert!(!footer.contains("开关mihomo"));
        assert!(!footer.contains("测速"));
        assert!(!footer.contains("TUN"));
        assert!(!footer.contains("切换面板"));
        let rows = help_rows(PageId::Main);
        assert!(rows.iter().any(|(_, d)| *d == "退出"));
        assert!(rows.iter().any(|(k, _)| k == "Enter"));
        assert!(rows.iter().any(|(_, d)| *d == "测速"));
        assert!(rows.iter().any(|(_, d)| *d == "开关mihomo"));
        assert!(rows.iter().any(|(_, d)| *d == "切换面板"));
    }

    #[test]
    fn test_key_label() {
        assert_eq!(key_label(KeyCode::Char('q')), "q");
        assert_eq!(key_label(KeyCode::Up), "↑");
        assert_eq!(key_label(KeyCode::Enter), "Enter");
    }

    #[test]
    fn test_binding_desc_filters_meta() {
        assert!(
            binding_meta(PageId::UrlInput, UrlInputPage::BINDINGS)
                .iter()
                .all(|b| !b.desc.is_empty())
        );
    }
}
