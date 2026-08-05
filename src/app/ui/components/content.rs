use crate::app::ui::ComponentEntry;
use crate::config::node::Node;
use ratatui::{
    layout::Constraint,
    style::{Color, Style},
    widgets::{Block, Borders, Cell, Row, Table},
};
use ui_derive::component;

/// 节点列表组件：节点数据 + 选中行
#[component(focusable)]
#[derive(Debug)]
pub struct Content {
    pub nodes: Vec<Node>,
    pub select: usize,
}

impl Content {
    pub fn new() -> Self {
        Self {
            nodes: vec![],
            select: 0,
        }
    }

    pub fn render(&self, focused: bool) -> Table<'_> {
        let rows: Vec<Row> = self
            .nodes
            .iter()
            .map(|node| {
                Row::new(vec![
                    Cell::from(node.name.clone()),
                    Cell::from(node.speed.clone()),
                ])
            })
            .collect();

        let header = Row::new(vec!["名称", "速度"])
            .style(Style::default().fg(Color::Yellow))
            .bottom_margin(1);

        let border = if focused { Color::Yellow } else { Color::DarkGray };

        Table::new(rows, [Constraint::Min(10), Constraint::Length(6)])
            .header(header)
            .block(
                Block::default()
                    .title("节点列表")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(border)),
            )
            .row_highlight_style(Style::default().bg(Color::LightBlue))
            .highlight_symbol(">> ")
    }
}
