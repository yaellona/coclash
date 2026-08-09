pub mod help;
pub mod main;
pub mod mihomo_log;
pub mod provider_select;
pub mod settings;
pub mod url_input;

use crate::app::App;
use crate::ui::Page;
use crossterm::event::KeyEvent;
use ratatui::Frame;

pub use help::HelpWindow;
pub use main::MainWindow;
pub use mihomo_log::MihomoLogWindow;
pub use provider_select::ProviderSelectWindow;
pub use settings::SettingsWindow;
pub use url_input::UrlInputWindow;

/// 窗口管理器：持有全部窗口与当前页，负责导航、按键分发与绘制。
pub struct Windows {
    pub current: Page,
    pub main: MainWindow,
    pub mihomo_log: MihomoLogWindow,
    pub help: HelpWindow,
    pub settings: SettingsWindow,
    pub url_input: UrlInputWindow,
    pub provider_select: ProviderSelectWindow,
}

impl Windows {
    pub fn new(app: &App) -> Self {
        Self {
            current: Page::Main,
            main: MainWindow::new(),
            mihomo_log: MihomoLogWindow::new(app),
            help: HelpWindow::new(),
            settings: SettingsWindow::new(),
            url_input: UrlInputWindow::new(),
            provider_select: ProviderSelectWindow::new(app),
        }
    }

    /// 导航到某页；弹窗打开时触发其 `on_open` 钩子
    pub fn open(&mut self, page: Page) {
        if page == self.current {
            return;
        }
        self.current = page;
        match page {
            Page::Help => self.help.on_open(),
            Page::Settings => self.settings.on_open(),
            Page::UrlInput => self.url_input.on_open(),
            Page::ProviderSelect => self.provider_select.on_open(),
            _ => {}
        }
    }

    /// 按键分发：窗口返回 `Some(page)` 表示请求导航
    pub fn handle_key(&mut self, app: &mut App, key: KeyEvent) {
        let nav = match self.current {
            Page::Main => self.main.handle_key(app, key),
            Page::MihomoLog => self.mihomo_log.handle_key(app, key),
            Page::Help => self.help.handle_key(app, key),
            Page::Settings => self.settings.handle_key(app, key),
            Page::UrlInput => self.url_input.handle_key(app, key),
            Page::ProviderSelect => self.provider_select.handle_key(app, key),
        };
        if let Some(page) = nav {
            self.open(page);
        }
    }

    /// 绘制：页面窗口独占全屏；弹窗作为覆盖层叠加在主窗口之上
    pub fn draw(&mut self, app: &mut App, f: &mut Frame) {
        match self.current {
            Page::Main => self.main.draw(app, f),
            Page::MihomoLog => self.mihomo_log.draw(app, f),
            Page::Help => {
                self.main.draw(app, f);
                self.help.draw(app, f);
            }
            Page::Settings => {
                self.main.draw(app, f);
                self.settings.draw(app, f);
            }
            Page::UrlInput => {
                self.main.draw(app, f);
                self.url_input.draw(app, f);
            }
            Page::ProviderSelect => {
                self.main.draw(app, f);
                self.provider_select.draw(app, f);
            }
        }
    }
}
