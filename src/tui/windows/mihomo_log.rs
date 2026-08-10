//! mihomo 进程日志窗口：tail 读取 + 统一 Scroller 滚动，长行自动换行。
use crate::manager::Manager;
use crate::constants::MIHOMO_LOG_FILE;
use crate::tui::Page;
use crate::tui::layout::wrap_lines;
use crate::tui::scroll::Scroller;
use crate::window;
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

pub struct MihomoLogWindow {
    path: PathBuf,
    /// 原始行
    lines: Vec<String>,
    /// 按显示宽度折行后的行
    rows: Vec<String>,
    scroller: Scroller,
    /// 上次绘制的可见行数（翻页用）
    visible: usize,
    last_size: u64,
}

#[window]
impl MihomoLogWindow {
    pub fn new(manager: &Manager) -> Self {
        Self {
            path: manager.config_dir().join(MIHOMO_LOG_FILE),
            lines: vec![],
            rows: vec![],
            scroller: Scroller::new(),
            visible: 1,
            last_size: 0,
        }
    }

    pub fn on_open(&mut self) {}

    /// 文件有更新时重新读取尾部，跟随模式自动滚到底部
    fn refresh(&mut self) {
        let size = std::fs::metadata(&self.path).map(|m| m.len()).unwrap_or(0);
        if size == self.last_size {
            return;
        }
        self.last_size = size;
        self.lines = read_tail(&self.path, MAX_TAIL_BYTES, MAX_TAIL_LINES);
    }

    #[key(KeyCode::Esc, "关闭", footer = false)]
    fn close(&mut self, _manager: &mut Manager) -> Option<Page> {
        Some(Page::Main)
    }

    #[key(KeyCode::Up, "导航", footer = false)]
    fn up(&mut self, _manager: &mut Manager) -> Option<Page> {
        self.scroller.up();
        None
    }

    #[key(KeyCode::Down, "导航", footer = false)]
    fn down(&mut self, _manager: &mut Manager) -> Option<Page> {
        let total = self.rows.len();
        self.scroller.down(total);
        None
    }

    #[key(KeyCode::PageUp, "翻页", footer = false)]
    fn page_up(&mut self, _manager: &mut Manager) -> Option<Page> {
        self.scroller.page_up(self.visible);
        None
    }

    #[key(KeyCode::PageDown, "翻页", footer = false)]
    fn page_down(&mut self, _manager: &mut Manager) -> Option<Page> {
        let total = self.rows.len();
        self.scroller.page_down(total, self.visible);
        None
    }

    pub fn draw(&mut self, _manager: &mut Manager, f: &mut Frame) {
        self.refresh();

        let area = f.area();
        let block = Block::default()
            .title("mihomo 进程日志 (Esc 关闭, ↑↓/PgUp/PgDn 滚动)")
            .borders(Borders::ALL)
            .style(Style::default().fg(Color::White));
        let inner = block.inner(area);
        f.render_widget(block, area);

        self.visible = inner.height.saturating_sub(1).max(1) as usize;
        self.rows = wrap_lines(&self.lines, inner.width.max(1) as usize);
        self.scroller.clamp(self.rows.len());
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
