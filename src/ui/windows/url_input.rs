//! 添加订阅窗口：输入框状态。
use crate::app::App;
use crate::ui::Page;
use crate::ui::layout::popup_rect;
use crate::window;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout},
    style::{Color, Style},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};

pub struct UrlInputWindow {
    pub input: String,
}

#[window(popup over Main)]
impl UrlInputWindow {
    pub fn new(_app: &App) -> Self {
        Self {
            input: String::new(),
        }
    }

    pub fn on_open(&mut self) {}

    #[key(KeyCode::Esc, "取消", footer = false)]
    fn cancel(&mut self, _app: &mut App) -> Option<Page> {
        self.input.clear();
        Some(Page::Main)
    }

    #[key(KeyCode::Enter, "确认", footer = false)]
    fn confirm(&mut self, app: &mut App) -> Option<Page> {
        if self.input.is_empty() {
            return None;
        }
        let url = self.input.clone();
        app.insert_sub(url);
        self.input.clear();
        Some(Page::Main)
    }

    #[key(KeyCode::Backspace, "删除字符", footer = false)]
    fn backspace(&mut self, _app: &mut App) -> Option<Page> {
        self.input.pop();
        None
    }

    #[key(KeyCode::Char(_))]
    fn input_char(&mut self, _app: &mut App, key: KeyEvent) -> Option<Page> {
        if let KeyCode::Char(c) = key.code {
            self.input.push(c);
        }
        None
    }

    pub fn draw(&mut self, _app: &mut App, f: &mut Frame) {
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
