use crate::app::Manager;
use crate::app::keymap::{Binding, help_rows, popup};
use crate::app::ui::{Popup, Window, WindowCtx};
use crate::app::ui::pages::popup_rect;
use crate::app::ui::pages::main::MAIN;
use crate::app::WindowId;
use crossterm::event::KeyCode;
use ratatui::{
    Frame,
    layout::Constraint,
    style::{Color, Style},
    widgets::{Block, Borders, Cell, Clear, Row, Table, TableState},
};
use std::sync::LazyLock;

/// 帮助窗口：表格滚动状态
pub struct HelpWindow {
    pub state: TableState,
}

impl HelpWindow {
    pub(crate) fn new(_ctx: &WindowCtx) -> Self {
        Self {
            state: TableState::default(),
        }
    }
}

#[popup(name = "help")]
impl HelpWindow {
    #[key(KeyCode::Esc, desc = "关闭")]
    fn key_close(&mut self, m: &mut Manager) {
        m.current_window = MAIN;
    }

    #[key(KeyCode::Up, desc = "导航")]
    fn key_help_up(&mut self, _m: &mut Manager) {
        let row = self.state.selected().unwrap_or(0).saturating_sub(1);
        self.state.select(Some(row));
    }

    #[key(KeyCode::Down, desc = "导航")]
    fn key_help_down(&mut self, _m: &mut Manager) {
        let len = help_rows(MAIN).len();
        let row = (self.state.selected().unwrap_or(0) + 1).min(len.saturating_sub(1));
        self.state.select(Some(row));
    }

    #[key(KeyCode::PageUp, desc = "翻页")]
    fn key_help_page_up(&mut self, _m: &mut Manager) {
        let visible = self.state.offset().max(1);
        let row = self.state.selected().unwrap_or(0).saturating_sub(visible);
        self.state.select(Some(row));
    }

    #[key(KeyCode::PageDown, desc = "翻页")]
    fn key_help_page_down(&mut self, _m: &mut Manager) {
        let len = help_rows(MAIN).len();
        let visible = self.state.offset().max(1);
        let row = (self.state.selected().unwrap_or(0) + visible).min(len.saturating_sub(1));
        self.state.select(Some(row));
    }

    /// 打开时回到第一行
    #[on_open]
    fn reset(&mut self, _m: &mut Manager) {
        self.state.select(Some(0));
    }

    #[render]
    fn draw(&mut self, _m: &mut Manager, f: &mut Frame) {
        let area = popup_rect(f.area());
        f.render_widget(Clear, area);
        let rows: Vec<Row> = help_rows(MAIN)
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

        f.render_stateful_widget(table, area, &mut self.state);
    }
}
