use ratatui::{
    style::{Color, Style},
    widgets::Paragraph,
};

pub fn render(shortcuts: &str) -> Paragraph<'_> {
    Paragraph::new(shortcuts).style(Style::default().fg(Color::White))
}
