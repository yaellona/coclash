use crate::app::keymap::{Binding, keymap};
use crate::app::{App, PopupMode};
use crate::app::ui::pages::centered_rect;
use crossterm::event::KeyCode;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout},
    style::{Color, Style},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};
use std::sync::LazyLock;

/// 添加订阅页面按键
#[keymap(name = "URL_INPUT_BINDINGS")]
impl App {
    #[key(KeyCode::Esc, mode = PopupMode::UrlInput, desc = "取消")]
    fn key_cancel_url_input(&mut self) {
        self.popup_mode = PopupMode::None;
        self.url_input.clear();
    }

    #[key(KeyCode::Enter, mode = PopupMode::UrlInput, desc = "确认")]
    fn key_submit_url(&mut self) {
        self.submit_url();
    }

    #[key(KeyCode::Backspace, mode = PopupMode::UrlInput, desc = "删除字符")]
    fn key_backspace(&mut self) {
        self.url_input.pop();
    }
}

pub fn draw(f: &mut Frame, app: &mut App) {
    let area = centered_rect(60, 20, f.area());

    f.render_widget(Clear, area);

    let block = Block::default()
        .title("添加订阅 (Enter 确认, Esc 取消)")
        .borders(Borders::ALL)
        .style(Style::default().fg(Color::White));

    let inner = block.inner(area);
    f.render_widget(block, area);

    let input_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1)])
        .split(inner);

    let input_text = if app.url_input.is_empty() {
        "请输入订阅 URL...".to_string()
    } else {
        format!("{}▌", app.url_input)
    };

    let style = if app.url_input.is_empty() {
        Style::default().fg(Color::DarkGray)
    } else {
        Style::default().fg(Color::White)
    };

    let input = Paragraph::new(input_text)
        .style(style)
        .wrap(Wrap { trim: false });

    f.render_widget(input, input_layout[0]);
}
