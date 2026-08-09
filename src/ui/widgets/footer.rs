//! 底部栏组件（只读展示）。
use ratatui::{
    style::{Color, Style},
    widgets::Paragraph,
};

pub struct Footer;

impl Footer {
    pub fn render<'a>(&self, shortcuts: &'a str) -> Paragraph<'a> {
        Paragraph::new(shortcuts).style(Style::default().fg(Color::White))
    }
}
