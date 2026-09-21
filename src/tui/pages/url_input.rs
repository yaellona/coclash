//! 添加订阅窗口：输入框状态。
use crate::manager::commands::Effect;
use crate::manager::state::AppState;
use crate::tui::action::{Binding, KeyPattern};
use crate::tui::cmd::{Cmd, Nav};
use crate::tui::layout::popup_rect;
use crate::tui::page::Page;
use crate::tui::pages::PageId;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout},
    style::{Color, Style},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};

pub struct UrlInputPage {
    pub input: String,
}

impl UrlInputPage {
    pub fn new() -> Self {
        Self {
            input: String::new(),
        }
    }

    fn cancel(&mut self, _: &AppState) -> Cmd {
        self.input.clear();
        Cmd::back()
    }

    fn confirm(&mut self, _: &AppState) -> Cmd {
        if self.input.is_empty() {
            return Cmd::none();
        }
        let url = self.input.clone();
        self.input.clear();
        Cmd::new(Nav::Back, Effect::InsertSub(url))
    }

    fn backspace(&mut self, _: &AppState) -> Cmd {
        self.input.pop();
        Cmd::none()
    }

    fn input_char(&mut self, _: &AppState, key: KeyEvent) -> Cmd {
        if let KeyCode::Char(c) = key.code {
            self.input.push(c);
        }
        Cmd::none()
    }
}

impl Default for UrlInputPage {
    fn default() -> Self {
        Self::new()
    }
}

impl Page for UrlInputPage {
    const BINDINGS: &'static [Binding<Self>] = &[
        Binding::on(KeyPattern::Code(KeyCode::Esc), "取消", false, Self::cancel),
        Binding::on(
            KeyPattern::Code(KeyCode::Enter),
            "确认",
            false,
            Self::confirm,
        ),
        Binding::on(
            KeyPattern::Code(KeyCode::Backspace),
            "删除字符",
            false,
            Self::backspace,
        ),
        Binding::text(KeyPattern::AnyChar, "", false, Self::input_char),
    ];

    fn parent(&self) -> Option<PageId> {
        Some(PageId::Main)
    }

    fn draw(&mut self, _state: &AppState, f: &mut Frame) {
        let area = popup_rect(f.area());

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

        let input_text = if self.input.is_empty() {
            "请输入订阅 URL...".to_string()
        } else {
            format!("{}▌", self.input)
        };

        let style = if self.input.is_empty() {
            Style::default().fg(Color::DarkGray)
        } else {
            Style::default().fg(Color::White)
        };

        let input = Paragraph::new(input_text)
            .style(style)
            .wrap(Wrap { trim: false });

        f.render_widget(input, input_layout[0]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::KeyModifiers;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn test_typing_and_confirm() {
        let state = AppState::test_fixture();
        let mut page = UrlInputPage::new();
        assert_eq!(
            page.handle_key(&state, key(KeyCode::Char('h'))),
            Cmd::none()
        );
        assert_eq!(
            page.handle_key(&state, key(KeyCode::Char('i'))),
            Cmd::none()
        );
        let cmd = page.handle_key(&state, key(KeyCode::Enter));
        assert_eq!(cmd, Cmd::new(Nav::Back, Effect::InsertSub("hi".into())));
        assert!(page.input.is_empty(), "确认后清空输入");
    }

    #[test]
    fn test_empty_confirm_is_noop() {
        let state = AppState::test_fixture();
        let mut page = UrlInputPage::new();
        assert_eq!(page.handle_key(&state, key(KeyCode::Enter)), Cmd::none());
    }

    #[test]
    fn test_cancel_clears_and_backs() {
        let state = AppState::test_fixture();
        let mut page = UrlInputPage::new();
        page.handle_key(&state, key(KeyCode::Char('a')));
        assert_eq!(page.handle_key(&state, key(KeyCode::Esc)), Cmd::back());
        assert!(page.input.is_empty());
    }
}
