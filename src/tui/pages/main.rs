//! 主页面：节点列表 + 操作日志 + 状态信息 + 底部栏。
use crate::manager::commands::Effect;
use crate::manager::state::AppState;
use crate::tui::action::{Binding, KeyPattern, footer_text};
use crate::tui::cmd::Cmd;
use crate::tui::page::Page;
use crate::tui::pages::PageId;
use crate::tui::widgets;
use crate::tui::widgets::OperationLog;
use crossterm::event::KeyCode;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout},
    widgets::TableState,
};

/// 主页面内的可聚焦面板（类型安全，穷尽匹配）
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
/// 状态信息区高度（6 行内容 + 上下边框）
const INFO_HEIGHT: u16 = 8;
/// 窄屏阈值
const NARROW_THRESHOLD: u16 = 70;

pub struct MainPage {
    pub focus: Panel,
    /// 节点光标：纯 UI 状态，不进 AppState（后台刷新不打断用户浏览位置）
    pub select: usize,
    pub operation_log: OperationLog,
}

impl MainPage {
    pub fn new() -> Self {
        Self {
            focus: Panel::Content,
            select: 0,
            operation_log: OperationLog::new(),
        }
    }

    fn navigate(&mut self, state: &AppState, step: i32) {
        let len = state.mihomo.nodes.len();
        if len == 0 {
            return;
        }
        self.select = (self.select as i32 + step).rem_euclid(len as i32) as usize;
    }

    /// 列表长度变化时收敛光标（绘制前调用）
    fn clamp_select(&mut self, state: &AppState) {
        self.select = self.select.min(state.mihomo.nodes.len().saturating_sub(1));
    }

    fn quit(&mut self, _: &AppState) -> Cmd {
        Cmd::quit()
    }

    fn open_help(&mut self, _: &AppState) -> Cmd {
        Cmd::goto(PageId::Help)
    }

    fn cycle_panel(&mut self, _: &AppState) -> Cmd {
        self.focus = self.focus.cycle();
        Cmd::none()
    }

    fn reset_focus(&mut self, _: &AppState) -> Cmd {
        self.focus = Panel::Content;
        Cmd::none()
    }

    fn up(&mut self, state: &AppState) -> Cmd {
        match self.focus {
            Panel::OperationLog => self.operation_log.up(),
            Panel::Content => self.navigate(state, -1),
        }
        Cmd::none()
    }

    fn down(&mut self, state: &AppState) -> Cmd {
        match self.focus {
            Panel::OperationLog => {
                let total = state.logs.len();
                self.operation_log.down(total);
            }
            Panel::Content => self.navigate(state, 1),
        }
        Cmd::none()
    }

    fn page_up(&mut self, _: &AppState) -> Cmd {
        if self.focus == Panel::OperationLog {
            self.operation_log.page_up();
        }
        Cmd::none()
    }

    fn page_down(&mut self, state: &AppState) -> Cmd {
        if self.focus == Panel::OperationLog {
            let total = state.logs.len();
            self.operation_log.page_down(total);
        }
        Cmd::none()
    }

    fn toggle_mihomo(&mut self, _: &AppState) -> Cmd {
        Cmd::effect(Effect::ToggleMihomo)
    }

    fn toggle_system_proxy(&mut self, _: &AppState) -> Cmd {
        Cmd::effect(Effect::ToggleSystemProxy)
    }

    fn toggle_tun(&mut self, _: &AppState) -> Cmd {
        Cmd::effect(Effect::ToggleTun)
    }

    fn provider_select(&mut self, _: &AppState) -> Cmd {
        Cmd::goto(PageId::ProviderSelect)
    }

    fn delay_test(&mut self, _: &AppState) -> Cmd {
        Cmd::effect(Effect::DelayTest)
    }

    fn refresh_nodes(&mut self, _: &AppState) -> Cmd {
        Cmd::effect(Effect::SyncNow)
    }

    fn add_subscription(&mut self, _: &AppState) -> Cmd {
        Cmd::goto(PageId::UrlInput)
    }

    fn open_log(&mut self, _: &AppState) -> Cmd {
        Cmd::goto(PageId::MihomoLog)
    }

    fn open_settings(&mut self, _: &AppState) -> Cmd {
        Cmd::goto(PageId::Settings)
    }

    fn select_node(&mut self, state: &AppState) -> Cmd {
        if self.focus == Panel::Content && !state.mihomo.nodes.is_empty() {
            Cmd::effect(Effect::SwitchNode(self.select))
        } else {
            Cmd::none()
        }
    }
}

impl Default for MainPage {
    fn default() -> Self {
        Self::new()
    }
}

impl Page for MainPage {
    const BINDINGS: &'static [Binding<Self>] = &[
        Binding::on(
            KeyPattern::Code(KeyCode::Char('q')),
            "退出",
            true,
            Self::quit,
        ),
        Binding::on(
            KeyPattern::Code(KeyCode::Char('?')),
            "帮助",
            true,
            Self::open_help,
        ),
        Binding::on(
            KeyPattern::Code(KeyCode::Tab),
            "切换面板",
            false,
            Self::cycle_panel,
        ),
        Binding::on(
            KeyPattern::Code(KeyCode::Esc),
            "回到节点列表",
            false,
            Self::reset_focus,
        ),
        Binding::on(KeyPattern::Code(KeyCode::Up), "导航", true, Self::up),
        Binding::on(KeyPattern::Code(KeyCode::Down), "导航", true, Self::down),
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
        Binding::on(
            KeyPattern::Code(KeyCode::Char('s')),
            "开关mihomo",
            false,
            Self::toggle_mihomo,
        ),
        Binding::on(
            KeyPattern::Code(KeyCode::Char('p')),
            "系统代理",
            false,
            Self::toggle_system_proxy,
        ),
        Binding::on(
            KeyPattern::Code(KeyCode::Char('T')),
            "TUN",
            false,
            Self::toggle_tun,
        ),
        Binding::on(
            KeyPattern::Code(KeyCode::Char('c')),
            "切换订阅",
            false,
            Self::provider_select,
        ),
        Binding::on(
            KeyPattern::Code(KeyCode::Char('t')),
            "测速",
            false,
            Self::delay_test,
        ),
        Binding::on(
            KeyPattern::Code(KeyCode::Char('r')),
            "刷新节点",
            false,
            Self::refresh_nodes,
        ),
        Binding::on(
            KeyPattern::Code(KeyCode::Char('u')),
            "添加订阅",
            false,
            Self::add_subscription,
        ),
        Binding::on(
            KeyPattern::Code(KeyCode::Char('l')),
            "mihomo日志",
            false,
            Self::open_log,
        ),
        Binding::on(
            KeyPattern::Code(KeyCode::Char('e')),
            "设置",
            true,
            Self::open_settings,
        ),
        Binding::on(
            KeyPattern::Code(KeyCode::Enter),
            "选中节点",
            false,
            Self::select_node,
        ),
    ];

    fn draw(&mut self, state: &AppState, f: &mut Frame) {
        let size = f.area();
        let footer_text = footer_text();
        let focus = self.focus;
        self.clamp_select(state);

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
            let info = widgets::RunningInfo::render(state);
            f.render_widget(info, chunks3[0]);

            // 折行宽度/可见高度都按「去掉边框和标签」的内区算：
            // 边框左右各 1 列 + 标签 "INFO " 6 列；上下边框 2 行
            let width = chunks2[1].width.saturating_sub(8).max(1) as usize;
            let height = chunks3[1].height.saturating_sub(2).max(1) as usize;
            self.operation_log.update(&state.logs, width, height);
            let log = self.operation_log.render(focus == Panel::OperationLog);
            f.render_widget(log, chunks3[1]);
        }

        let select = self.select;
        let content = widgets::Content::render(&state.mihomo.nodes, focus == Panel::Content);
        f.render_stateful_widget(
            &content,
            chunks2[0],
            &mut TableState::default().with_selected(Some(select)),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyEvent, KeyModifiers};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn test_panel_cycle() {
        assert_eq!(Panel::Content.cycle(), Panel::OperationLog);
        assert_eq!(Panel::OperationLog.cycle(), Panel::Content);
    }

    #[test]
    fn test_handlers_return_cmds() {
        let state = AppState::test_fixture();
        let mut page = MainPage::new();
        assert_eq!(
            page.handle_key(&state, key(KeyCode::Char('t'))),
            Cmd::effect(Effect::DelayTest)
        );
        assert_eq!(
            page.handle_key(&state, key(KeyCode::Char('s'))),
            Cmd::effect(Effect::ToggleMihomo)
        );
        assert_eq!(
            page.handle_key(&state, key(KeyCode::Char('?'))),
            Cmd::goto(PageId::Help)
        );
        assert_eq!(
            page.handle_key(&state, key(KeyCode::Char('q'))),
            Cmd::quit()
        );
    }

    #[test]
    fn test_select_node_requires_nodes_and_content_focus() {
        let mut state = AppState::test_fixture();
        let mut page = MainPage::new();
        assert_eq!(page.handle_key(&state, key(KeyCode::Enter)), Cmd::none());

        state
            .mihomo
            .nodes
            .push(crate::manager::state::Node::new("a".into()));
        state
            .mihomo
            .nodes
            .push(crate::manager::state::Node::new("b".into()));
        page.focus = Panel::Content;
        assert_eq!(page.handle_key(&state, key(KeyCode::Down)), Cmd::none());
        assert_eq!(page.select, 1);
        assert_eq!(
            page.handle_key(&state, key(KeyCode::Enter)),
            Cmd::effect(Effect::SwitchNode(1))
        );

        page.focus = Panel::OperationLog;
        assert_eq!(page.handle_key(&state, key(KeyCode::Enter)), Cmd::none());
    }
}
