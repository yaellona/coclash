use crate::app::keymap::{Binding, help_rows, keymap};
use crate::app::{App, PopupMode};
use crate::app::ui::pages::centered_rect;
use crossterm::event::KeyCode;
use ratatui::{
    Frame,
    layout::Constraint,
    style::{Color, Style},
    widgets::{Block, Borders, Cell, Clear, Row, Table},
};
use std::sync::LazyLock;

/// 帮助页面按键
#[keymap(name = "HELP_BINDINGS")]
impl App {
    #[key(KeyCode::Esc, mode = PopupMode::HelpKey, desc = "关闭")]
    fn key_close_help(&mut self) {
        self.popup_mode = PopupMode::None;
    }

    #[key(KeyCode::Up, mode = PopupMode::HelpKey, desc = "导航")]
    fn key_help_up(&mut self) {
        self.help_scroll_up();
    }

    #[key(KeyCode::Down, mode = PopupMode::HelpKey, desc = "导航")]
    fn key_help_down(&mut self) {
        self.help_scroll_down();
    }

    #[key(KeyCode::PageUp, mode = PopupMode::HelpKey, desc = "翻页")]
    fn key_help_page_up(&mut self) {
        self.help_page_up();
    }

    #[key(KeyCode::PageDown, mode = PopupMode::HelpKey, desc = "翻页")]
    fn key_help_page_down(&mut self) {
        self.help_page_down();
    }
}

pub fn draw(f: &mut Frame, app: &mut App) {
    let area = centered_rect(50, 40, f.area());
    f.render_widget(Clear, area);
    let rows: Vec<Row> = help_rows(PopupMode::None)
        .into_iter()
        .map(|(key, desc)| {
            Row::new(vec![Cell::from(key), Cell::from(desc.to_string())])
        })
        .collect();

    let table = Table::new(rows, [Constraint::Length(10), Constraint::Min(0)])
        .highlight_symbol(">> ")
        .row_highlight_style(Style::default().bg(Color::LightBlue))
        .block(
            Block::default()
                .title("帮助")
                .title_bottom("ESC退出，↑↓导航")
                .borders(Borders::ALL),
        );

    f.render_stateful_widget(table, area, &mut app.help_state);
}
