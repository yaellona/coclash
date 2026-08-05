use crate::app::ui::ComponentEntry;
use ratatui::{
    style::{Color, Style},
    widgets::Paragraph,
};
use ui_derive::component;

/// 底部栏组件（只读展示）
#[component]
#[derive(Debug)]
pub struct Footer;

impl Footer {
    pub fn render<'a>(&self, shortcuts: &'a str) -> Paragraph<'a> {
        Paragraph::new(shortcuts).style(Style::default().fg(Color::White))
    }
}
