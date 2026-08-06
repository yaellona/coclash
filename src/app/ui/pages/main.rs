use crate::app::Manager;
use crate::app::keymap::{Binding, footer_text, window};
use crate::app::tasks::{self, switch_node};
use crate::app::ui::{Window, WindowCtx};
use crate::app::ui::components::{Content, Footer, OperationLog, RunningInfo};
use crate::app::{Panel, WindowId};
use crate::operation_log::LogType;
use crossterm::event::KeyCode;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout},
};
use std::sync::LazyLock;

/// 主窗口：持有其全部组件（节点列表/操作日志/状态信息/底部栏）与面板焦点
pub struct MainWindow {
    pub content: Content,
    pub operation_log: OperationLog,
    pub running_info: RunningInfo,
    pub footer: Footer,
    pub focus: Panel,
}

impl MainWindow {
    pub(crate) fn new(_ctx: &WindowCtx) -> Self {
        Self {
            content: Content::new(),
            operation_log: OperationLog::new(),
            running_info: RunningInfo,
            footer: Footer,
            focus: Panel::Content,
        }
    }

    fn navigate(&mut self, step: i32) {
        let len = self.content.nodes.len();
        if len == 0 {
            return;
        }
        self.content.select = wrap(self.content.select, len, step);
    }

    fn start_delay_test(&mut self, m: &mut Manager) {
        if m.mihomo.is_test_delay {
            m.logs.add(LogType::Warn, "已经在测速了!".to_string());
            return;
        }
        m.mihomo.is_test_delay = true;
        for node in &mut self.content.nodes {
            node.speed = "wait".to_string();
        }
        tasks::delay(m.tasks.tx.clone(), m.config.settings.clone());
    }
}

/// 选择下标回绕（独立纯函数便于测试）
fn wrap(select: usize, len: usize, step: i32) -> usize {
    (select as i32 + step).rem_euclid(len as i32) as usize
}

/// 主窗口：本页按键都在这里注册（`#[window]` 自动生成
/// `MAIN` ID 常量、`MAIN_BINDINGS` 元数据表、`impl Window`）
#[window(name = "main")]
impl MainWindow {
    #[key('q', desc = "退出", footer)]
    fn key_quit(m: &mut Manager) {
        m.should_quit = true;
    }

    #[key('?', desc = "帮助", footer)]
    fn key_open_help(m: &mut Manager) {
        m.current_window = super::help::HELP;
    }

    #[key(KeyCode::Tab, desc = "切换面板")]
    fn key_switch_focus(&mut self, _m: &mut Manager) {
        self.focus = self.focus.cycle();
    }

    #[key(KeyCode::Esc, desc = "回到节点列表")]
    fn key_reset_focus(&mut self, _m: &mut Manager) {
        self.focus = Panel::Content;
    }

    #[key(KeyCode::Up, desc = "导航", footer)]
    fn key_node_up(&mut self, m: &mut Manager) {
        match self.focus {
            Panel::OperationLog => self.operation_log.scroll_up(&m.logs.items),
            Panel::Content => self.navigate(-1),
        }
    }

    #[key(KeyCode::Down, desc = "导航", footer)]
    fn key_node_down(&mut self, m: &mut Manager) {
        match self.focus {
            Panel::OperationLog => self.operation_log.scroll_down(&m.logs.items),
            Panel::Content => self.navigate(1),
        }
    }

    #[key(KeyCode::PageUp, desc = "翻页")]
    fn key_page_up(&mut self, m: &mut Manager) {
        if self.focus == Panel::OperationLog {
            self.operation_log.page_up(&m.logs.items);
        }
    }

    #[key(KeyCode::PageDown, desc = "翻页")]
    fn key_page_down(&mut self, m: &mut Manager) {
        if self.focus == Panel::OperationLog {
            self.operation_log.page_down(&m.logs.items);
        }
    }

    #[key('s', desc = "开关mihomo")]
    fn key_toggle_mihomo(m: &mut Manager) {
        m.toggle_mihomo();
    }

    #[key('p', desc = "系统代理")]
    fn key_toggle_proxy(m: &mut Manager) {
        m.toggle_system_proxy();
    }

    #[key('T', desc = "TUN")]
    fn key_toggle_tun(m: &mut Manager) {
        m.toggle_tun();
    }

    #[key('c', desc = "切换代理")]
    fn key_open_agency_select(m: &mut Manager) {
        m.current_window = super::provider_select::PROVIDER_SELECT;
    }

    #[key('t', desc = "测速")]
    fn key_delay_test(&mut self, m: &mut Manager) {
        self.start_delay_test(m);
    }

    #[key('r', desc = "刷新节点")]
    fn key_refresh_nodes(m: &mut Manager) {
        m.load_nodes();
    }

    #[key('u', desc = "添加订阅")]
    fn key_open_url_input(m: &mut Manager) {
        m.current_window = super::url_input::URL_INPUT;
    }

    #[key('l', desc = "mihomo日志")]
    fn key_open_mihomo_log(m: &mut Manager) {
        m.current_window = super::mihomo_log::MIHOMO_LOG;
    }

    #[key('e', desc = "设置", footer)]
    fn key_open_settings(m: &mut Manager) {
        m.current_window = super::settings::SETTINGS;
    }

    #[key(KeyCode::Enter, desc = "选中节点")]
    fn key_select_node(&mut self, m: &mut Manager) {
        if self.focus != Panel::Content {
            return;
        }
        if !self.content.nodes.is_empty() {
            let index = self.content.select;
            m.mihomo.active_node = Some(index);
            let name = self.content.nodes[index].name.clone();
            switch_node(m.tasks.tx.clone(), m.config.settings.clone(), name);
        }
    }

    /// 主窗口绘制：底部栏 + 左节点列表 + 右状态信息/操作日志
    #[render]
    fn draw(&mut self, m: &mut Manager, f: &mut Frame) {
        let size = f.area();
        let footer_text = footer_text();
        let focus = self.focus;

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

        f.render_widget(self.footer.render(&footer_text), main_chunks[1]);
        if constraint.len() > 1 {
            let chunks3 = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(7), Constraint::Min(0)])
                .split(chunks2[1]);
            let info = self.running_info.render(m, &self.content.nodes);
            f.render_widget(info, chunks3[0]);
            let log = OperationLog::render(
                &m.logs.items,
                chunks2[1].width as usize - 10,
                focus == Panel::OperationLog,
            );
            self.operation_log.clamp_follow(&m.logs.items);
            f.render_stateful_widget(log, chunks3[1], &mut self.operation_log.state);
        }

        let select = self.content.select;
        let content = self.content.render(focus == Panel::Content);
        f.render_stateful_widget(
            &content,
            chunks2[0],
            &mut ratatui::widgets::TableState::default().with_selected(Some(select)),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::wrap;

    #[test]
    fn test_wrap_around() {
        assert_eq!(wrap(0, 2, 1), 1);
        assert_eq!(wrap(1, 2, 1), 0);
        assert_eq!(wrap(0, 2, -1), 1);
        assert_eq!(wrap(1, 2, -1), 0);
        assert_eq!(wrap(0, 1, 5), 0);
    }
}
