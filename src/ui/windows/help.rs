//! 帮助窗口：统一 Scroller + Paragraph 渲染。
use crate::app::App;
use crate::ui::Page;
use crate::ui::keymap::{Binding, help_rows};
use crate::ui::layout::popup_rect;
use crate::ui::scroll::Scroller;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    Frame,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

pub const BINDINGS: &[Binding] = &[
    Binding {
        mode: Page::Help,
        key: KeyCode::Esc,
        desc: "关闭",
        in_footer: false,
    },
    Binding {
        mode: Page::Help,
        key: KeyCode::Up,
        desc: "导航",
        in_footer: false,
    },
    Binding {
        mode: Page::Help,
        key: KeyCode::Down,
        desc: "导航",
        in_footer: false,
    },
    Binding {
        mode: Page::Help,
        key: KeyCode::PageUp,
        desc: "翻页",
        in_footer: false,
    },
    Binding {
        mode: Page::Help,
        key: KeyCode::PageDown,
        desc: "翻页",
        in_footer: false,
    },
];

pub struct HelpWindow {
    scroller: Scroller,
    /// 上次绘制的可见行数（翻页用）
    visible: usize,
}

impl HelpWindow {
    pub fn new() -> Self {
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

    pub fn handle_key(&mut self, _app: &mut App, key: KeyEvent) -> Option<Page> {
        let total = help_rows(Page::Main).len();
        match key.code {
            KeyCode::Esc => Some(Page::Main),
            KeyCode::Up => {
                self.scroller.up();
                None
            }
            KeyCode::Down => {
                self.scroller.down(total);
                None
            }
            KeyCode::PageUp => {
                self.scroller.page_up(self.visible);
                None
            }
            KeyCode::PageDown => {
                self.scroller.page_down(total, self.visible);
                None
            }
            _ => None,
        }
    }

    pub fn draw(&mut self, _app: &mut App, f: &mut Frame) {
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
