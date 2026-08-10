//! 主窗口：节点列表 + 操作日志 + 状态信息 + 底部栏。
use crate::app::App;
use crate::ui::keymap::footer_text;
use crate::ui::widgets::OperationLog;
use crate::ui::{Page, widgets};
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
    /// 上次绘制时的操作日志可见高度（翻页用）
    log_visible: usize,
}

#[window]
impl MainWindow {
    pub fn new(_app: &App) -> Self {
        Self {
            focus: Panel::Content,
            operation_log: OperationLog::new(),
            log_visible: 1,
        }
    }

    pub fn on_open(&mut self) {}

    fn navigate(&mut self, app: &mut App, step: i32) {
        let len = app.state.nodes.len();
        if len == 0 {
            return;
        }
        app.state.select = wrap(app.state.select, len, step);
    }

    #[key(KeyCode::Char('q'), "退出", footer = true)]
    fn quit(&mut self, app: &mut App) -> Option<Page> {
        app.should_quit = true;
        None
    }

    #[key(KeyCode::Char('?'), "帮助", footer = true)]
    fn open_help(&mut self, _app: &mut App) -> Option<Page> {
        Some(Page::Help)
    }

    #[key(KeyCode::Tab, "切换面板", footer = false)]
    fn cycle_panel(&mut self, _app: &mut App) -> Option<Page> {
        self.focus = self.focus.cycle();
        None
    }

    #[key(KeyCode::Esc, "回到节点列表", footer = false)]
    fn reset_focus(&mut self, _app: &mut App) -> Option<Page> {
        self.focus = Panel::Content;
        None
    }

    #[key(KeyCode::Up, "导航", footer = true)]
    fn up(&mut self, app: &mut App) -> Option<Page> {
        match self.focus {
            Panel::OperationLog => self.operation_log.up(),
            Panel::Content => self.navigate(app, -1),
        }
        None
    }

    #[key(KeyCode::Down, "导航", footer = true)]
    fn down(&mut self, app: &mut App) -> Option<Page> {
        match self.focus {
            Panel::OperationLog => {
                let total = app.state.logs.len();
                self.operation_log.down(total);
            }
            Panel::Content => self.navigate(app, 1),
        }
        None
    }

    #[key(KeyCode::PageUp, "翻页", footer = false)]
    fn page_up(&mut self, _app: &mut App) -> Option<Page> {
        if self.focus == Panel::OperationLog {
            self.operation_log.page_up(self.log_visible);
        }
        None
    }

    #[key(KeyCode::PageDown, "翻页", footer = false)]
    fn page_down(&mut self, app: &mut App) -> Option<Page> {
        if self.focus == Panel::OperationLog {
            let total = app.state.logs.len();
            self.operation_log.page_down(total, self.log_visible);
        }
        None
    }

    #[key(KeyCode::Char('s'), "开关mihomo", footer = false)]
    fn toggle_mihomo(&mut self, app: &mut App) -> Option<Page> {
        app.toggle_mihomo();
        None
    }

    #[key(KeyCode::Char('p'), "系统代理", footer = false)]
    fn toggle_system_proxy(&mut self, app: &mut App) -> Option<Page> {
        app.toggle_system_proxy();
        None
    }

    #[key(KeyCode::Char('T'), "TUN", footer = false)]
    fn toggle_tun(&mut self, app: &mut App) -> Option<Page> {
        app.toggle_tun();
        None
    }

    #[key(KeyCode::Char('c'), "切换代理", footer = false)]
    fn provider_select(&mut self, _app: &mut App) -> Option<Page> {
        Some(Page::ProviderSelect)
    }

    #[key(KeyCode::Char('t'), "测速", footer = false)]
    fn delay_test(&mut self, app: &mut App) -> Option<Page> {
        app.start_delay_test();
        None
    }

    #[key(KeyCode::Char('r'), "刷新节点", footer = false)]
    fn refresh_nodes(&mut self, app: &mut App) -> Option<Page> {
        app.load_nodes();
        None
    }

    #[key(KeyCode::Char('u'), "添加订阅", footer = false)]
    fn add_subscription(&mut self, _app: &mut App) -> Option<Page> {
        Some(Page::UrlInput)
    }

    #[key(KeyCode::Char('l'), "mihomo日志", footer = false)]
    fn open_log(&mut self, _app: &mut App) -> Option<Page> {
        Some(Page::MihomoLog)
    }

    #[key(KeyCode::Char('e'), "设置", footer = true)]
    fn open_settings(&mut self, _app: &mut App) -> Option<Page> {
        Some(Page::Settings)
    }

    #[key(KeyCode::Enter, "选中节点", footer = false)]
    fn select_node(&mut self, app: &mut App) -> Option<Page> {
        if self.focus == Panel::Content && !app.state.nodes.is_empty() {
            let index = app.state.select;
            app.switch_node(index);
        }
        None
    }

    pub fn draw(&mut self, app: &mut App, f: &mut Frame) {
        let size = f.area();
        let footer_text = footer_text();
        let focus = self.focus;

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
            let info = widgets::RunningInfo::render(&app.state);
            f.render_widget(info, chunks3[0]);

            let width = chunks2[1].width.saturating_sub(10).max(1) as usize;
            let height = chunks3[1].height.max(1) as usize;
            self.log_visible = height;
            let log = self.operation_log.render(
                &app.state.logs,
                width,
                height,
                focus == Panel::OperationLog,
            );
            f.render_widget(log, chunks3[1]);
        }

        let select = app.state.select;
        let content = widgets::Content::render(&app.state.nodes, focus == Panel::Content);
        f.render_stateful_widget(
            &content,
            chunks2[0],
            &mut TableState::default().with_selected(Some(select)),
        );
    }
}

/// 选择下标回绕（独立纯函数便于测试）
fn wrap(select: usize, len: usize, step: i32) -> usize {
    (select as i32 + step).rem_euclid(len as i32) as usize
}

#[cfg(test)]
mod tests {
    use super::{Panel, wrap};

    #[test]
    fn test_wrap_around() {
        assert_eq!(wrap(0, 2, 1), 1);
        assert_eq!(wrap(1, 2, 1), 0);
        assert_eq!(wrap(0, 2, -1), 1);
        assert_eq!(wrap(1, 2, -1), 0);
        assert_eq!(wrap(0, 1, 5), 0);
    }

    #[test]
    fn test_panel_cycle() {
        assert_eq!(Panel::Content.cycle(), Panel::OperationLog);
        assert_eq!(Panel::OperationLog.cycle(), Panel::Content);
    }
}
