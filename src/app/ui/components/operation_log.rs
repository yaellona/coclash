use crate::app::ui::ComponentEntry;
use crate::operation_log::{LogType, OperationLogs};
use ratatui::{
    layout::Constraint,
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Cell, Row, Table, TableState},
};
use ui_derive::component;

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

/// 操作日志组件：滚动视图状态（日志数据在 OperationLogManager，作为共享数据传入）
#[component(focusable)]
#[derive(Debug)]
pub struct OperationLog {
    pub state: TableState,
    pub follow: bool,
}

impl OperationLog {
    pub fn new() -> Self {
        Self {
            state: TableState::default(),
            follow: true,
        }
    }

    /// 当前选中行（0 起），日志为空时为 0
    fn selected(&self, logs: &OperationLogs) -> usize {
        self.state
            .selected()
            .unwrap_or(logs.len().saturating_sub(1))
            .min(logs.len().saturating_sub(1))
    }

    pub fn scroll_up(&mut self, logs: &OperationLogs) {
        self.follow = false;
        let row = self.selected(logs).saturating_sub(1);
        self.state.select(Some(row));
    }

    pub fn scroll_down(&mut self, logs: &OperationLogs) {
        let max = logs.len().saturating_sub(1);
        let row = (self.selected(logs) + 1).min(max);
        self.state.select(Some(row));
        if row == max {
            self.follow = true;
        }
    }

    pub fn page_up(&mut self, logs: &OperationLogs) {
        self.follow = false;
        let visible = self.state.offset().max(1);
        let row = self.selected(logs).saturating_sub(visible);
        self.state.select(Some(row));
    }

    pub fn page_down(&mut self, logs: &OperationLogs) {
        let max = logs.len().saturating_sub(1);
        let visible = self.state.offset().max(1);
        let row = (self.selected(logs) + visible).min(max);
        self.state.select(Some(row));
        if row == max {
            self.follow = true;
        }
    }

    /// 绘制时调用：跟随模式下始终贴底；否则仅在日志缩短导致选中越界时钳制
    pub fn clamp_follow(&mut self, logs: &OperationLogs) {
        if logs.is_empty() {
            return;
        }
        let max = logs.len() - 1;
        if self.follow || self.state.selected().is_some_and(|s| s > max) {
            self.state.select(Some(max));
        }
    }

    pub fn render(logs: &OperationLogs, width: usize, focused: bool) -> Table<'_> {
        let rows: Vec<Row> = logs
            .iter()
            .map(|log| {
                let wrapped = wrap_text(&log.msg, width);
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
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_logs(n: usize) -> OperationLogs {
        let mut logs = OperationLogs::new();
        for i in 0..n {
            logs.add_log(LogType::Info, format!("msg {i}"));
        }
        logs
    }

    #[test]
    fn test_scroll_leaves_follow_and_back() {
        let mut log = OperationLog::new();
        let logs = sample_logs(5);

        assert!(log.follow);
        log.scroll_up(&logs);
        assert!(!log.follow);
        assert_eq!(log.state.selected(), Some(3));

        log.scroll_down(&logs);
        assert!(log.follow);
        assert_eq!(log.state.selected(), Some(4));
    }

    #[test]
    fn test_scroll_empty_logs() {
        let mut log = OperationLog::new();
        let logs = OperationLogs::new();
        log.scroll_up(&logs);
        log.scroll_down(&logs);
        assert_eq!(log.state.selected(), Some(0));
    }

    #[test]
    fn test_page_down_bounds() {
        let mut log = OperationLog::new();
        let logs = sample_logs(3);
        log.page_down(&logs);
        assert!(log.follow);
        assert_eq!(log.state.selected(), Some(2));
    }
}
