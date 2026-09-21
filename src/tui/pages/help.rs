//! 帮助窗口：统一 Scroller + Paragraph 渲染。
use crate::manager::state::AppState;
use crate::tui::action::{Binding, KeyPattern, help_rows};
use crate::tui::cmd::Cmd;
use crate::tui::layout::popup_rect;
use crate::tui::page::Page;
use crate::tui::pages::PageId;
use crate::tui::scroll::Scroller;
use crossterm::event::KeyCode;
use ratatui::{
    Frame,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

pub struct HelpPage {
    scroller: Scroller,
    /// 上次绘制的可见行数（翻页用）
    visible: usize,
}

impl HelpPage {
    pub fn new() -> Self {
        Self {
            scroller: Scroller::new(),
            visible: 1,
        }
    }

    /// 帮助内容展示父页面的按键（当前为 Main，未来换父页自动跟随）
    fn target(&self) -> PageId {
        self.parent().unwrap_or(PageId::Main)
    }

    fn close(&mut self, _: &AppState) -> Cmd {
        Cmd::back()
    }

    fn up(&mut self, _: &AppState) -> Cmd {
        self.scroller.up();
        Cmd::none()
    }

    fn down(&mut self, _: &AppState) -> Cmd {
        let total = help_rows(self.target()).len();
        self.scroller.down(total);
        Cmd::none()
    }

    fn page_up(&mut self, _: &AppState) -> Cmd {
        self.scroller.page_up(self.visible);
        Cmd::none()
    }

    fn page_down(&mut self, _: &AppState) -> Cmd {
        let total = help_rows(self.target()).len();
        self.scroller.page_down(total, self.visible);
        Cmd::none()
    }
}

impl Default for HelpPage {
    fn default() -> Self {
        Self::new()
    }
}

impl Page for HelpPage {
    const BINDINGS: &'static [Binding<Self>] = &[
        Binding::on(KeyPattern::Code(KeyCode::Esc), "关闭", false, Self::close),
        Binding::on(KeyPattern::Code(KeyCode::Up), "导航", false, Self::up),
        Binding::on(KeyPattern::Code(KeyCode::Down), "导航", false, Self::down),
        Binding::on(
            KeyPattern::Code(KeyCode::PageUp),
            "翻页",
            false,
            Self::page_up,
        ),
        Binding::on(
            KeyPattern::Code(KeyCode::PageDown),
            "翻页",
            false,
            Self::page_down,
        ),
    ];

    fn parent(&self) -> Option<PageId> {
        Some(PageId::Main)
    }

    fn on_open(&mut self) {
        self.scroller.select = 0;
        self.scroller.follow = false;
    }

    fn draw(&mut self, _state: &AppState, f: &mut Frame) {
        let area = popup_rect(f.area());
        f.render_widget(Clear, area);

        let rows = help_rows(self.target());

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

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyEvent, KeyModifiers};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn test_close_returns_back() {
        let state = AppState::test_fixture();
        let mut page = HelpPage::new();
        assert_eq!(page.handle_key(&state, key(KeyCode::Esc)), Cmd::back());
        assert_eq!(
            page.handle_key(&state, key(KeyCode::Char('x'))),
            Cmd::none()
        );
    }
}
