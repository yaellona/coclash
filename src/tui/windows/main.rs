//! 主窗口：节点列表 + 操作日志 + 状态信息 + 底部栏。
use crate::manager::Manager;
use crate::tui::keymap::footer_text;
use crate::tui::widgets::OperationLog;
use crate::tui::{Page, widgets};
use crate::window;
use crossterm::event::KeyCode;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout},
    widgets::TableState,
};

/// 主窗口内的可聚焦面板（类型安全，穷尽匹配）
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Panel {
    Content,
    OperationLog,
}

impl Panel {
    pub fn cycle(&self) -> Panel {
        match self {
            Panel::Content => Panel::OperationLog,
            Panel::OperationLog => Panel::Content,
        }
    }
}

/// 侧边栏宽度
const SIDEBAR_WIDTH: u16 = 50;
/// 状态信息区高度
const INFO_HEIGHT: u16 = 7;
/// 窄屏阈值
const NARROW_THRESHOLD: u16 = 70;

pub struct MainWindow {
    pub focus: Panel,
    pub operation_log: OperationLog,
}

#[window]
impl MainWindow {
    pub fn new(_manager: &Manager) -> Self {
        Self {
            focus: Panel::Content,
            operation_log: OperationLog::new(),
        }
    }

    pub fn on_open(&mut self) {}

    fn navigate(&mut self, manager: &Manager, step: i32) {
        manager.state_lock().navigate(step);
    }

    #[key(KeyCode::Char('q'), "退出", footer = true)]
    fn quit(&mut self, manager: &Manager) -> Option<Page> {
        manager.should_quit.store(true, std::sync::atomic::Ordering::Relaxed);
        None
    }

    #[key(KeyCode::Char('?'), "帮助", footer = true)]
    fn open_help(&mut self, _manager: &Manager) -> Option<Page> {
        Some(Page::Help)
    }

    #[key(KeyCode::Tab, "切换面板", footer = false)]
    fn cycle_panel(&mut self, _manager: &Manager) -> Option<Page> {
        self.focus = self.focus.cycle();
        None
    }

    #[key(KeyCode::Esc, "回到节点列表", footer = false)]
    fn reset_focus(&mut self, _manager: &Manager) -> Option<Page> {
        self.focus = Panel::Content;
        None
    }

    #[key(KeyCode::Up, "导航", footer = true)]
    fn up(&mut self, manager: &Manager) -> Option<Page> {
        match self.focus {
            Panel::OperationLog => self.operation_log.up(),
            Panel::Content => self.navigate(manager, -1),
        }
        None
    }

    #[key(KeyCode::Down, "导航", footer = true)]
    fn down(&mut self, manager: &Manager) -> Option<Page> {
        match self.focus {
            Panel::OperationLog => {
                let total = manager.state_lock().logs.len();
                self.operation_log.down(total);
            }
            Panel::Content => self.navigate(manager, 1),
        }
        None
    }

    #[key(KeyCode::PageUp, "翻页", footer = false)]
    fn page_up(&mut self, _manager: &Manager) -> Option<Page> {
        if self.focus == Panel::OperationLog {
            self.operation_log.page_up();
        }
        None
    }

    #[key(KeyCode::PageDown, "翻页", footer = false)]
    fn page_down(&mut self, manager: &Manager) -> Option<Page> {
        if self.focus == Panel::OperationLog {
            let total = manager.state_lock().logs.len();
            self.operation_log.page_down(total);
        }
        None
    }

    #[key(KeyCode::Char('s'), "开关mihomo", footer = false)]
    fn toggle_mihomo(&mut self, manager: &Manager) -> Option<Page> {
        manager.toggle_mihomo();
        None
    }

    #[key(KeyCode::Char('p'), "系统代理", footer = false)]
    fn toggle_system_proxy(&mut self, manager: &Manager) -> Option<Page> {
        manager.toggle_system_proxy();
        None
    }

    #[key(KeyCode::Char('T'), "TUN", footer = false)]
    fn toggle_tun(&mut self, manager: &Manager) -> Option<Page> {
        manager.toggle_tun();
        None
    }

    #[key(KeyCode::Char('c'), "切换订阅", footer = false)]
    fn provider_select(&mut self, _manager: &Manager) -> Option<Page> {
        Some(Page::ProviderSelect)
    }

    #[key(KeyCode::Char('t'), "测速", footer = false)]
    fn delay_test(&mut self, manager: &Manager) -> Option<Page> {
        manager.start_delay_test();
        None
    }

    #[key(KeyCode::Char('r'), "刷新节点", footer = false)]
    fn refresh_nodes(&mut self, manager: &Manager) -> Option<Page> {
        manager.load_nodes();
        None
    }

    #[key(KeyCode::Char('u'), "添加订阅", footer = false)]
    fn add_subscription(&mut self, _manager: &Manager) -> Option<Page> {
        Some(Page::UrlInput)
    }

    #[key(KeyCode::Char('l'), "mihomo日志", footer = false)]
    fn open_log(&mut self, _manager: &Manager) -> Option<Page> {
        Some(Page::MihomoLog)
    }

    #[key(KeyCode::Char('e'), "设置", footer = true)]
    fn open_settings(&mut self, _manager: &Manager) -> Option<Page> {
        Some(Page::Settings)
    }

    #[key(KeyCode::Enter, "选中节点", footer = false)]
    fn select_node(&mut self, manager: &Manager) -> Option<Page> {
        let (has_nodes, index) = {
            let st = manager.state_lock();
            (!st.nodes.is_empty(), st.select)
        };
        if self.focus == Panel::Content && has_nodes {
            manager.switch_node(index);
        }
        None
    }

    pub fn draw(&mut self, manager: &Manager, f: &mut Frame) {
        let size = f.area();
        let footer_text = footer_text();
        let focus = self.focus;
        let state = manager.state_lock();

        //底部快捷键区域和其他区域
        let main_chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Min(0), Constraint::Length(1)])
            .split(size);

        let constraint = if size.width > NARROW_THRESHOLD {
            vec![Constraint::Min(40), Constraint::Length(SIDEBAR_WIDTH)]
        } else {
            vec![Constraint::Min(40)]
        };
        //左右两部分区域
        let chunks2 = Layout::default()
            .direction(Direction::Horizontal)
            .constraints(&constraint)
            .split(main_chunks[0]);

        f.render_widget(widgets::Footer.render(&footer_text), main_chunks[1]);

        if constraint.len() > 1 {
            let chunks3 = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(INFO_HEIGHT), Constraint::Min(0)])
                .split(chunks2[1]);
            let info = widgets::RunningInfo::render(&state);
            f.render_widget(info, chunks3[0]);

            let width = chunks2[1].width.saturating_sub(10).max(1) as usize;
            let height = chunks3[1].height.max(1) as usize;
            self.operation_log.update(&state.logs, width, height);
            let log = self.operation_log.render(focus == Panel::OperationLog);
            f.render_widget(log, chunks3[1]);
        }

        let select = state.select;
        let content = widgets::Content::render(&state.nodes, focus == Panel::Content);
        f.render_stateful_widget(
            &content,
            chunks2[0],
            &mut TableState::default().with_selected(Some(select)),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::Panel;

    #[test]
    fn test_panel_cycle() {
        assert_eq!(Panel::Content.cycle(), Panel::OperationLog);
        assert_eq!(Panel::OperationLog.cycle(), Panel::Content);
    }
}
