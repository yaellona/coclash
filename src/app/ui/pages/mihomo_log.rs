use crate::app::Manager;
use crate::app::keymap::{Binding, window};
use crate::app::mihomo_log::MihomoLogView;
use crate::app::ui::{Window, WindowCtx};
use crate::app::ui::pages::main::MAIN;
use crate::app::WindowId;
use crossterm::event::KeyCode;
use ratatui::{
    Frame,
    style::{Color, Modifier, Style},
    widgets::{Block, Borders, Paragraph},
};
use std::sync::LazyLock;

/// mihomo 进程日志窗口：全屏页面，长行自动换行
pub struct MihomoLogWindow {
    pub view: MihomoLogView,
}

impl MihomoLogWindow {
    pub(crate) fn new(ctx: &WindowCtx) -> Self {
        Self {
            view: MihomoLogView::new(ctx.config_dir.join(crate::constants::MIHOMO_LOG_FILE)),
        }
    }
}

#[window(name = "mihomo_log")]
impl MihomoLogWindow {
    #[key(KeyCode::Esc, desc = "关闭")]
    fn key_close(&mut self, m: &mut Manager) {
        m.current_window = MAIN;
    }

    #[key(KeyCode::Up, desc = "导航")]
    fn key_log_up(&mut self, _m: &mut Manager) {
        self.view.scroll_up();
    }

    #[key(KeyCode::Down, desc = "导航")]
    fn key_log_down(&mut self, _m: &mut Manager) {
        self.view.scroll_down();
    }

    #[key(KeyCode::PageUp, desc = "翻页")]
    fn key_log_page_up(&mut self, _m: &mut Manager) {
        self.view.page_up();
    }

    #[key(KeyCode::PageDown, desc = "翻页")]
    fn key_log_page_down(&mut self, _m: &mut Manager) {
        self.view.page_down();
    }

    #[render]
    fn draw(&mut self, _m: &mut Manager, f: &mut Frame) {
        let view = &mut self.view;
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
        view.wrap(inner.width.max(1) as usize);
        view.clamp_scroll();

        let text = if view.rows.is_empty() {
            "（暂无日志，启动 mihomo 后自动生成）".to_string()
        } else {
            let start = view.scroll.min(view.rows.len() - 1);
            let end = (start + view.visible).min(view.rows.len());
            view.rows[start..end].join("\n")
        };

        let paragraph = Paragraph::new(text)
            .style(Style::default().fg(Color::Gray).add_modifier(Modifier::DIM));
        f.render_widget(paragraph, inner);
    }
}
