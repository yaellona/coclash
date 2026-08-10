//! 帮助窗口：统一 Scroller + Paragraph 渲染。
use crate::manager::Manager;
use crate::tui::Page;
use crate::tui::keymap::help_rows;
use crate::tui::layout::popup_rect;
use crate::tui::scroll::Scroller;
use crate::window;
use crossterm::event::KeyCode;
use ratatui::{
    Frame,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

pub struct HelpWindow {
    scroller: Scroller,
    /// 上次绘制的可见行数（翻页用）
    visible: usize,
}

#[window(popup over Main)]
impl HelpWindow {
    pub fn new(_manager: &Manager) -> Self {
        Self {
            scroller: Scroller::new(),
            visible: 1,
        }
    }

    /// 打开时回到第一行
    pub fn on_open(&mut self) {
        self.scroller.select = 0;
        self.scroller.follow = false;
    }

    #[key(KeyCode::Esc, "关闭", footer = false)]
    fn close(&mut self, _manager: &mut Manager) -> Option<Page> {
        Some(Page::Main)
    }

    #[key(KeyCode::Up, "导航", footer = false)]
    fn up(&mut self, _manager: &mut Manager) -> Option<Page> {
        self.scroller.up();
        None
    }

    #[key(KeyCode::Down, "导航", footer = false)]
    fn down(&mut self, _manager: &mut Manager) -> Option<Page> {
        let total = help_rows(Page::Main).len();
        self.scroller.down(total);
        None
    }

    #[key(KeyCode::PageUp, "翻页", footer = false)]
    fn page_up(&mut self, _manager: &mut Manager) -> Option<Page> {
        self.scroller.page_up(self.visible);
        None
    }

    #[key(KeyCode::PageDown, "翻页", footer = false)]
    fn page_down(&mut self, _manager: &mut Manager) -> Option<Page> {
        let total = help_rows(Page::Main).len();
        self.scroller.page_down(total, self.visible);
        None
    }

    pub fn draw(&mut self, _manager: &mut Manager, f: &mut Frame) {
        let area = popup_rect(f.area());
        f.render_widget(Clear, area);

        let rows = help_rows(Page::Main);

        let block = Block::default()
            .title("帮助")
            .title_bottom("ESC退出，↑↓导航")
            .borders(Borders::ALL)
            .style(Style::default().fg(Color::White));
        let inner = block.inner(area);
        f.render_widget(block, area);

        self.visible = inner.height.max(1) as usize;
        self.scroller.clamp(rows.len());
        let (start, end) = self.scroller.viewport(rows.len(), self.visible);

        let lines: Vec<Line> = rows
            .iter()
            .enumerate()
            .skip(start)
            .take(end.saturating_sub(start))
            .map(|(i, (key, desc))| {
                let style = if i == self.scroller.select {
                    Style::default().bg(Color::LightBlue)
                } else {
                    Style::default().fg(Color::White)
                };
                Line::from(vec![
                    Span::styled(format!("{key:<10}"), style),
                    Span::styled(desc.to_string(), style),
                ])
            })
            .collect();

        f.render_widget(Paragraph::new(lines), inner);
    }
}
