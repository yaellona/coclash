//! 文本测量/折行与弹窗布局工具。
use ratatui::layout::{Constraint, Direction, Layout, Rect};

/// 简单显示宽度：ASCII 1，其余按 2（本项目文案只有 ASCII + CJK）
pub fn display_width(s: &str) -> usize {
    s.chars().map(|c| if c.is_ascii() { 1 } else { 2 }).sum()
}

/// 选择下标环绕：`select` 以 `len` 为模移动 `step`（负步长向前，正步长向后）。
/// 调用方需保证 `len > 0`（列表为空时先短路）。
pub fn wrap_index(select: usize, len: usize, step: i32) -> usize {
    (select as i32 + step).rem_euclid(len as i32) as usize
}

/// 按显示宽度逐字折行，返回展平后的行列表（空行保留为空字符串）
pub fn wrap_lines(lines: &[String], width: usize) -> Vec<String> {
    let mut out = Vec::new();
    for line in lines {
        if width == 0 {
            out.push(line.clone());
            continue;
        }
        let mut current = String::new();
        let mut used = 0;
        for c in line.chars() {
            let cw = if c.is_ascii() { 1 } else { 2 };
            if used + cw > width && !current.is_empty() {
                out.push(current);
                current = String::new();
                used = 0;
            }
            current.push(c);
            used += cw;
        }
        out.push(current);
    }
    out
}

/// 弹窗尺寸常量：70% 宽 × 60% 高
const POPUP_WIDTH: u16 = 70;
const POPUP_HEIGHT: u16 = 60;

/// 全部弹窗统一尺寸，如需调整只改这里
pub fn popup_rect(r: Rect) -> Rect {
    centered_rect(POPUP_WIDTH, POPUP_HEIGHT, r)
}

/// 弹窗共用：居中区域
pub fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_display_width() {
        assert_eq!(display_width("SOCKS 端口"), 10);
        assert_eq!(display_width("混合端口"), 8);
        assert_eq!(display_width("abc"), 3);
    }

    #[test]
    fn test_wrap_index() {
        assert_eq!(wrap_index(0, 2, 1), 1);
        assert_eq!(wrap_index(1, 2, 1), 0);
        assert_eq!(wrap_index(0, 2, -1), 1);
        assert_eq!(wrap_index(1, 2, -1), 0);
        assert_eq!(wrap_index(0, 1, 5), 0);
    }

    #[test]
    fn test_wrap_lines_ascii() {
        let lines = vec!["abcdef".to_string()];
        assert_eq!(wrap_lines(&lines, 3), vec!["abc", "def"]);
    }

    #[test]
    fn test_wrap_lines_cjk() {
        let lines = vec!["中文测试".to_string()];
        assert_eq!(wrap_lines(&lines, 4), vec!["中文", "测试"]);
    }

    #[test]
    fn test_wrap_lines_mixed() {
        let lines = vec!["ab中c".to_string()];
        assert_eq!(wrap_lines(&lines, 4), vec!["ab中", "c"]);
    }

    #[test]
    fn test_wrap_lines_narrow() {
        let lines = vec!["ab".to_string()];
        assert_eq!(wrap_lines(&lines, 1), vec!["a", "b"]);
    }

    #[test]
    fn test_wrap_lines_empty_line_preserved() {
        let lines = vec!["a".to_string(), "".to_string(), "b".to_string()];
        assert_eq!(wrap_lines(&lines, 10), vec!["a", "", "b"]);
    }

    #[test]
    fn test_wrap_lines_zero_width() {
        let lines = vec!["abc".to_string()];
        assert_eq!(wrap_lines(&lines, 0), vec!["abc"]);
    }

    #[test]
    fn test_wrap_lines_exact_fit() {
        let lines = vec!["abcd".to_string()];
        assert_eq!(wrap_lines(&lines, 4), vec!["abcd"]);
    }
}
