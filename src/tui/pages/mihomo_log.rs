//! mihomo 进程日志窗口：tail 读取 + 统一 Scroller 滚动，长行自动换行。
use crate::manager::state::AppState;
use crate::tui::action::{Binding, KeyPattern};
use crate::tui::cmd::Cmd;
use crate::tui::layout::wrap_lines;
use crate::tui::page::Page;
use crate::tui::scroll::Scroller;
use crossterm::event::KeyCode;
use ratatui::{
    Frame,
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Paragraph},
};
use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;

const MAX_TAIL_BYTES: u64 = 128 * 1024;
const MAX_TAIL_LINES: usize = 500;

pub struct MihomoLogPage {
    path: PathBuf,
    /// 原始行
    lines: Vec<String>,
    /// 按显示宽度折行后的行
    rows: Vec<String>,
    scroller: Scroller,
    /// 上次绘制的可见行数（翻页用）
    visible: usize,
    last_size: u64,
    /// 上次折行时的显示宽度（宽度/文件未变则跳过重折行）
    last_width: usize,
}

impl MihomoLogPage {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            lines: vec![],
            rows: vec![],
            scroller: Scroller::new(),
            visible: 1,
            last_size: 0,
            last_width: 0,
        }
    }

    /// 文件有更新时重新读取尾部，跟随模式自动滚到底部；返回是否更新
    fn refresh(&mut self) -> bool {
        let size = std::fs::metadata(&self.path).map(|m| m.len()).unwrap_or(0);
        if size == self.last_size {
            return false;
        }
        self.last_size = size;
        self.lines = read_tail(&self.path, MAX_TAIL_BYTES, MAX_TAIL_LINES);
        true
    }

    fn close(&mut self, _: &AppState) -> Cmd {
        Cmd::back()
    }

    fn up(&mut self, _: &AppState) -> Cmd {
        self.scroller.up();
        Cmd::none()
    }

    fn down(&mut self, _: &AppState) -> Cmd {
        let total = self.rows.len();
        self.scroller.down(total);
        Cmd::none()
    }

    fn page_up(&mut self, _: &AppState) -> Cmd {
        self.scroller.page_up(self.visible);
        Cmd::none()
    }

    fn page_down(&mut self, _: &AppState) -> Cmd {
        let total = self.rows.len();
        self.scroller.page_down(total, self.visible);
        Cmd::none()
    }

    /// 每帧渲染前调用：读文件更新 + 折行 + 收敛滚动。
    /// 状态副作用集中在此，`draw` 其余部分保持只读渲染。
    fn update(&mut self, width: usize, height: usize) {
        let changed = self.refresh();
        // 传入的是去掉边框的内区高度，直接作为可见行数（此前多减 1 浪费一行）
        self.visible = height.max(1);
        if changed || width != self.last_width {
            self.rows = wrap_lines(&self.lines, width.max(1));
            self.last_width = width;
        }
        self.scroller.clamp(self.rows.len());
    }
}

impl Page for MihomoLogPage {
    const BINDINGS: &'static [Binding<Self>] = &[
        Binding::on(KeyPattern::Code(KeyCode::Esc), "关闭", false, Self::close),
        Binding::on(KeyPattern::Code(KeyCode::Up), "导航", false, Self::up),
        Binding::on(KeyPattern::Code(KeyCode::Down), "导航", false, Self::down),
        Binding::on(
            KeyPattern::Code(KeyCode::PageUp),
            "翻页",
            false,
            Self::page_up,
        ),
        Binding::on(
            KeyPattern::Code(KeyCode::PageDown),
            "翻页",
            false,
            Self::page_down,
        ),
    ];

    fn draw(&mut self, _state: &AppState, f: &mut Frame) {
        let area = f.area();
        let block = Block::default()
            .title("mihomo 进程日志 (Esc 关闭, ↑↓/PgUp/PgDn 滚动)")
            .borders(Borders::ALL)
            .style(Style::default().fg(Color::White));
        let inner = block.inner(area);
        f.render_widget(block, area);

        self.update(inner.width as usize, inner.height as usize);
        let (start, end) = self.scroller.viewport(self.rows.len(), self.visible);

        let text = if self.rows.is_empty() {
            "（暂无日志，启动 mihomo 后自动生成）".to_string()
        } else {
            self.rows[start..end].join("\n")
        };

        let paragraph = Paragraph::new(text)
            .style(Style::default().fg(Color::Gray).add_modifier(Modifier::DIM));
        f.render_widget(paragraph, inner);
    }
}

/// 读取文件尾部（bytes 与行数双上限）
pub(crate) fn read_tail(path: &std::path::Path, max_bytes: u64, max_lines: usize) -> Vec<String> {
    let Ok(mut file) = std::fs::File::open(path) else {
        return vec![];
    };
    let len = file.metadata().map(|m| m.len()).unwrap_or(0);
    let start = len.saturating_sub(max_bytes);
    let from_middle = start > 0;
    if from_middle {
        let _ = file.seek(SeekFrom::Start(start));
    }
    let mut buf = Vec::new();
    if file.read_to_end(&mut buf).is_err() {
        return vec![];
    }
    let mut lines: Vec<String> = String::from_utf8_lossy(&buf)
        .lines()
        .map(|s| s.to_string())
        .collect();
    if from_middle {
        lines.remove(0);
    }
    if lines.len() > max_lines {
        lines.drain(0..lines.len() - max_lines);
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_read_tail() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("mihomo.log");
        let mut f = std::fs::File::create(&path).unwrap();
        for i in 0..100 {
            writeln!(f, "line {i}").unwrap();
        }
        drop(f);
        let lines = read_tail(&path, 128 * 1024, 10);
        assert_eq!(lines.len(), 10);
        assert_eq!(lines[0], "line 90");
        assert_eq!(lines[9], "line 99");
    }
}
