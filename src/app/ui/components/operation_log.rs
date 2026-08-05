use crate::operation_log::{LogType, OperationLogs};
use ratatui::{
    layout::Constraint,
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Cell, Row, Table},
};

fn wrap_text(text: &str, width: usize) -> String {
    text.chars()
        .collect::<Vec<_>>()
        .chunks(width)
        .map(|chunk| chunk.iter().collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}

// 类型标签：强色，Error 加粗
fn tag_style(t: &LogType) -> Style {
    let color = match t {
        LogType::Info => Color::LightBlue,
        LogType::Warn => Color::Yellow,
        LogType::Error => Color::Red,
    };
    let mut s = Style::default().fg(color);
    if matches!(t, LogType::Error) {
        s = s.add_modifier(Modifier::BOLD);
    }
    s
}

// 正文：同色系的淡变体 + DIM，营造"淡染"
fn body_style(t: &LogType) -> Style {
    let color = match t {
        LogType::Info => Color::LightBlue,
        LogType::Warn => Color::LightYellow,
        LogType::Error => Color::LightRed,
    };
    Style::default().fg(color).add_modifier(Modifier::DIM)
}

pub fn render(logs: &OperationLogs, width: usize, focused: bool) -> Table<'_> {
    let rows: Vec<Row> = logs
        .iter()
        .map(|log| {
            let wrapped = wrap_text(&log.msg, width as usize);
            let line_count = wrapped.lines().count();

            Row::new(vec![
                Cell::from(log.log_type.as_str()).style(tag_style(&log.log_type)),
                Cell::from(wrapped).style(body_style(&log.log_type)),
            ])
            .height(line_count as u16) // 设置行高为换行后的行数
        })
        .collect();

    let border = if focused { Color::Yellow } else { Color::DarkGray };

    Table::new(rows, [Constraint::Length(5), Constraint::Min(0)])
        .highlight_symbol("")
        .row_highlight_style(Style::default())
        .block(
            Block::default()
                .title("操作记录")
                .borders(Borders::ALL)
                .border_style(Style::default().fg(border)),
        )
}
