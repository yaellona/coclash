use crate::app::keymap::{Binding, keymap};
use crate::app::{App, PopupMode};
use crate::app::ui::pages::centered_rect;
use crossterm::event::KeyCode;
use ratatui::{
    Frame,
    style::{Color, Style},
    widgets::{Block, Borders, Clear, Paragraph},
};
use std::sync::LazyLock;

/// 选择代理商页面按键
#[keymap(name = "PROVIDER_SELECT_BINDINGS")]
impl App {
    #[key(KeyCode::Esc, mode = PopupMode::AgencySelect, desc = "取消")]
    fn key_close_agency_select(&mut self) {
        self.popup_mode = PopupMode::None;
    }

    #[key(KeyCode::Up, mode = PopupMode::AgencySelect, desc = "导航")]
    fn key_provider_up(&mut self) {
        self.navigate_provider(-1);
    }

    #[key(KeyCode::Down, mode = PopupMode::AgencySelect, desc = "导航")]
    fn key_provider_down(&mut self) {
        self.navigate_provider(1);
    }

    #[key('d', mode = PopupMode::AgencySelect, desc = "删除代理")]
    fn key_delete_provider(&mut self) {
        self.delete_current_provider();
    }

    #[key(KeyCode::Enter, mode = PopupMode::AgencySelect, desc = "确认")]
    fn key_confirm_provider(&mut self) {
        if let Some(name) = self.config.provider_key_by_index(self.select_provider) {
            self.switch_provider(name);
        }
    }
}

pub fn draw(f: &mut Frame, app: &mut App) {
    let area = centered_rect(60, 40, f.area());

    // 清除背景
    f.render_widget(Clear, area);

    let block = Block::default()
        .title("选择代理商")
        .title_bottom("(Enter 确认, Esc 取消, d 删除代理)")
        .borders(Borders::ALL)
        .style(Style::default().fg(Color::White));

    let inner = block.inner(area);
    f.render_widget(block, area);

    // 构建代理商列表
    let items: Vec<String> = app
        .config
        .proxy_providers
        .as_ref()
        .map(|providers| {
            providers
                .keys() // 获取所有 key
                .enumerate()
                .map(|(i, key)| {
                    let marker = if i == app.select_provider {
                        ">> "
                    } else {
                        "   "
                    };
                    format!("{}{}", marker, key)
                })
                .collect()
        })
        .unwrap_or_default();

    let list_text = items.join("\n");

    let style = Style::default().fg(Color::White);

    let list = Paragraph::new(list_text).style(style);

    f.render_widget(list, inner);
}
