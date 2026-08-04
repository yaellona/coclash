pub mod content;
pub mod footer;
pub mod mihomo_log;
pub mod operation_log;
pub mod popup;
pub mod running_info;
use crate::app::PopupMode;

use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout},
};

use crate::app::App;
impl App {
    pub fn draw(&mut self, f: &mut Frame) {
        let size = f.area();
        let footer_text = format!("q: 退出 | ↑↓: 导航 | ?: 查看帮助");

        let footer = footer::render(&footer_text);

        //底部快捷键区域和其他区域
        let main_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(0),
                //footer区域
                Constraint::Length(1),
            ])
            .split(size);

        let constraint = if size.width > 70 {
            vec![Constraint::Min(40), Constraint::Length(50)]
        } else {
            vec![Constraint::Min(40)]
        };
        //左右两部分区域
        let chunks2 = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(&constraint)
            .split(main_chunks[0]);
        // 侧边栏

        let content = content::render(&self.current_nodes);
        f.render_widget(footer, main_chunks[1]);
        if constraint.len() > 1 {
            let chunks3 = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(7), Constraint::Min(0)])
                .split(chunks2[1]);
            let info = running_info::render(&self);
            f.render_widget(info, chunks3[0]);
            let log = operation_log::render(&self.logs, chunks2[1].width as usize - 10);
            if !self.logs.is_empty() {
                self.log_state.select(Some(self.logs.len() - 1));
            }
            f.render_stateful_widget(log, chunks3[1], &mut self.log_state);
        }

        f.render_stateful_widget(
            &content,
            chunks2[0],
            &mut ratatui::widgets::TableState::default().with_selected(Some(self.select_node)),
        );

        // 弹窗渲染
        match self.popup_mode {
            PopupMode::UrlInput => {
                popup::render_url_input(f, self);
            }
            PopupMode::AgencySelect => {
                popup::render_provider_select(f, self);
            }
            PopupMode::HelpKey => {
                popup::help_key(f, self);
            }
            PopupMode::MihomoLog => {
                mihomo_log::render(f, self);
            }
            _ => {}
        }
    }
}
