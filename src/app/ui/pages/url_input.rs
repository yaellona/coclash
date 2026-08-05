use crate::app::Manager;
use crate::app::keymap::{Binding, popup};
use crate::app::tasks;
use crate::app::ui::{Popup, Window, WindowCtx};
use crate::app::ui::pages::centered_rect;
use crate::app::ui::pages::main::MAIN;
use crate::app::WindowId;
use crate::operation_log::LogType;
use crossterm::event::KeyCode;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout},
    style::{Color, Style},
    widgets::{Block, Borders, Clear, Paragraph, Wrap},
};
use std::sync::LazyLock;

/// 添加订阅窗口：输入框状态
pub struct UrlInputWindow {
    pub input: String,
}

impl UrlInputWindow {
    pub(crate) fn new(_ctx: &WindowCtx) -> Self {
        Self { input: String::new() }
    }

    fn submit(&mut self, m: &mut Manager) {
        if self.input.is_empty() {
            return;
        }
        let url = self.input.clone();
        m.current_window = MAIN;
        self.input.clear();
        m.logs.add(LogType::Info, "正在验证URL...".to_string());
        tasks::insert_sub(m.tasks.tx.clone(), m.config.settings.clone(), url);
    }
}

#[popup(name = "url_input")]
impl UrlInputWindow {
    #[key(KeyCode::Esc, desc = "取消")]
    fn key_cancel(&mut self, m: &mut Manager) {
        m.current_window = MAIN;
        self.input.clear();
    }

    #[key(KeyCode::Enter, desc = "确认")]
    fn key_submit(&mut self, m: &mut Manager) {
        self.submit(m);
    }

    #[key(KeyCode::Backspace, desc = "删除字符")]
    fn key_backspace(&mut self, _m: &mut Manager) {
        self.input.pop();
    }

    #[fallback]
    fn key_type(&mut self, _m: &mut Manager, key: KeyCode) {
        if let KeyCode::Char(c) = key {
            self.input.push(c);
        }
    }

    #[render]
    fn draw(&mut self, _m: &mut Manager, f: &mut Frame) {
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
