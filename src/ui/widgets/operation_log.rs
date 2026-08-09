//! 操作日志组件：统一 Scroller + Paragraph 渲染（按显示宽度折行）。
use crate::operation_log::{LogType, OperationLogs};
use crate::ui::layout::wrap_lines;
use crate::ui::scroll::Scroller;
use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Paragraph},
};

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

pub struct OperationLog {
    scroller: Scroller,
}

impl OperationLog {
    pub fn new() -> Self {
        Self {
            scroller: Scroller::new(),
        }
    }

    pub fn up(&mut self) {
        self.scroller.up();
    }

    pub fn down(&mut self, total: usize) {
        self.scroller.down(total);
    }

    pub fn page_up(&mut self, visible: usize) {
        self.scroller.page_up(visible);
    }

    pub fn page_down(&mut self, total: usize, visible: usize) {
        self.scroller.page_down(total, visible);
    }

    /// 日志行展平为 (类型, 显示行)：每条日志按宽度折行，标签逐行重复
    fn rows(logs: &OperationLogs, width: usize) -> Vec<(LogType, String)> {
        let mut rows = Vec::new();
        for log in logs.iter() {
            let wrapped = wrap_lines(std::slice::from_ref(&log.msg), width);
            for line in wrapped {
                rows.push((log.log_type.clone(), line));
            }
        }
        rows
    }

    pub fn render(
        &mut self,
        logs: &OperationLogs,
        width: usize,
        height: usize,
        focused: bool,
    ) -> Paragraph<'static> {
        let rows = Self::rows(logs, width);
        self.scroller.clamp(rows.len());
        let visible = height.max(1);
        let (start, end) = self.scroller.viewport(rows.len(), visible);

        let lines: Vec<Line> = rows
            .iter()
            .enumerate()
            .skip(start)
            .take(end.saturating_sub(start))
            .map(|(i, (t, text))| {
                let spans = vec![
                    Span::styled(format!("{:<5}", t.as_str()), tag_style(t)),
                    Span::styled(" ", Style::default()),
                    Span::styled(text.clone(), body_style(t)),
                ];
                let mut line = Line::from(spans);
                if i == self.scroller.select {
                    line = line.patch_style(Style::default().bg(Color::DarkGray));
                }
                line
            })
            .collect();

        let border = if focused {
            Color::Yellow
        } else {
            Color::DarkGray
        };

        Paragraph::new(lines)
            .block(
                Block::default()
                    .title("操作记录")
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(border)),
            )
            .style(Style::default().fg(Color::White))
    }
}

impl Default for OperationLog {
    fn default() -> Self {
        Self::new()
    }
}
