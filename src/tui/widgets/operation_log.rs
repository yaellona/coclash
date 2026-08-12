//! 操作日志组件：统一 Scroller + Paragraph 渲染（按显示宽度折行）。
use crate::operation_log::{LogType, OperationLogs};
use crate::tui::layout::wrap_lines;
use crate::tui::scroll::Scroller;
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
    /// 折行后的显示行（update 生成，render 只读）
    rows: Vec<(LogType, String)>,
    /// 可见高度（update 记录，翻页用）
    visible: usize,
}

impl OperationLog {
    pub fn new() -> Self {
        Self {
            scroller: Scroller::new(),
            rows: vec![],
            visible: 1,
        }
    }

    pub fn up(&mut self) {
        self.scroller.up();
    }

    pub fn down(&mut self, total: usize) {
        self.scroller.down(total);
    }

    pub fn page_up(&mut self) {
        self.scroller.page_up(self.visible);
    }

    pub fn page_down(&mut self, total: usize) {
        self.scroller.page_down(total, self.visible);
    }

    /// 每帧渲染前调用：按当前宽度折行并收敛滚动位置。
    /// 状态副作用集中在此，`render` 保持只读（渲染不得改状态）。
    pub fn update(&mut self, logs: &OperationLogs, width: usize, height: usize) {
        self.rows = Self::rows(logs, width);
        self.visible = height.max(1);
        self.scroller.clamp(self.rows.len());
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

    /// 只读渲染：行数据与滚动位置由 `update` 提供
    pub fn render(&self, focused: bool) -> Paragraph<'static> {
        let rows = &self.rows;
        let (start, end) = self.scroller.viewport(rows.len(), self.visible);

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
