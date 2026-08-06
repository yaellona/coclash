use crate::app::ui::pages::wrap_lines;
use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;
const MAX_TAIL_BYTES: u64 = 128 * 1024;
const MAX_TAIL_LINES: usize = 500;

/// mihomo 进程日志视图状态：tail 读取 + 滚动（按换行后的显示行计）
#[derive(Debug)]
pub struct MihomoLogView {
    path: PathBuf,
    lines: Vec<String>,
    pub rows: Vec<String>,
    pub scroll: usize,
    pub visible: usize,
    pub follow: bool,
    last_size: u64,
}

impl MihomoLogView {
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            lines: vec![],
            rows: vec![],
            scroll: 0,
            visible: 1,
            follow: true,
            last_size: 0,
        }
    }

    /// 文件有更新时重新读取尾部，跟随模式自动滚到底部
    pub fn refresh(&mut self) {
        let size = std::fs::metadata(&self.path)
            .map(|m| m.len())
            .unwrap_or(0);
        if size == self.last_size {
            return;
        }
        self.last_size = size;
        self.lines = read_tail(&self.path, MAX_TAIL_BYTES, MAX_TAIL_LINES);
    }

    /// 按当前显示宽度把原始行折行为显示行
    pub fn wrap(&mut self, width: usize) {
        self.rows = wrap_lines(&self.lines, width);
    }

    /// 根据可见行数把 scroll 收敛到合法范围
    pub fn clamp_scroll(&mut self) {
        let max = self.rows.len().saturating_sub(self.visible);
        if self.follow {
            self.scroll = max;
        } else {
            self.scroll = self.scroll.min(max);
        }
    }

    pub fn scroll_up(&mut self) {
        self.follow = false;
        self.scroll = self.scroll.saturating_sub(1);
    }

    pub fn scroll_down(&mut self) {
        let max = self.rows.len().saturating_sub(self.visible);
        self.scroll = (self.scroll + 1).min(max);
        if self.scroll == max {
            self.follow = true;
        }
    }

    pub fn page_up(&mut self) {
        self.follow = false;
        self.scroll = self.scroll.saturating_sub(self.visible.max(1));
    }

    pub fn page_down(&mut self) {
        let max = self.rows.len().saturating_sub(self.visible);
        self.scroll = (self.scroll + self.visible.max(1)).min(max);
        if self.scroll == max {
            self.follow = true;
        }
    }
}

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
