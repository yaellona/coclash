use crate::app::keymap::{Binding, keymap};
use crate::app::{App, PopupMode};
use crossterm::event::KeyCode;
use ratatui::{
    Frame,
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Paragraph},
};
use std::sync::LazyLock;

/// mihomo 进程日志页面按键
#[keymap(name = "MIHOMO_LOG_BINDINGS")]
impl App {
    #[key(KeyCode::Esc, mode = PopupMode::MihomoLog, desc = "关闭")]
    fn key_close_mihomo_log(&mut self) {
        self.popup_mode = PopupMode::None;
    }

    #[key(KeyCode::Up, mode = PopupMode::MihomoLog, desc = "导航")]
    fn key_log_up(&mut self) {
        self.mihomo_log.scroll_up();
    }

    #[key(KeyCode::Down, mode = PopupMode::MihomoLog, desc = "导航")]
    fn key_log_down(&mut self) {
        self.mihomo_log.scroll_down();
    }

    #[key(KeyCode::PageUp, mode = PopupMode::MihomoLog, desc = "翻页")]
    fn key_log_page_up(&mut self) {
        self.mihomo_log.page_up();
    }

    #[key(KeyCode::PageDown, mode = PopupMode::MihomoLog, desc = "翻页")]
    fn key_log_page_down(&mut self) {
        self.mihomo_log.page_down();
    }
}

pub fn draw(f: &mut Frame, app: &mut App) {
    let view = &mut app.mihomo_log;
    view.refresh();

    let area = f.area();
    let block = Block::default()
        .title("mihomo 进程日志 (Esc 关闭, ↑↓/PgUp/PgDn 滚动)")
        .borders(Borders::ALL)
        .style(Style::default().fg(Color::White));
    let inner = block.inner(area);
    f.render_widget(block, area);

    view.visible = inner.height.saturating_sub(1) as usize;
    view.visible = view.visible.max(1);
    view.clamp_scroll();

    let text = if view.lines.is_empty() {
        "（暂无日志，启动 mihomo 后自动生成）".to_string()
    } else {
        let start = view.scroll.min(view.lines.len() - 1);
        let end = (start + view.visible).min(view.lines.len());
        view.lines[start..end].join("\n")
    };

    let paragraph = Paragraph::new(text)
        .style(Style::default().fg(Color::Gray).add_modifier(Modifier::DIM));
    f.render_widget(paragraph, inner);
}
