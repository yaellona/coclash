//! 主窗口：节点列表 + 操作日志 + 状态信息 + 底部栏。
use crate::app::App;
use crate::ui::keymap::{Binding, footer_text};
use crate::ui::widgets::OperationLog;
use crate::ui::{Page, widgets};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout},
    widgets::TableState,
};

pub const BINDINGS: &[Binding] = &[
    Binding {
        mode: Page::Main,
        key: KeyCode::Char('q'),
        desc: "退出",
        in_footer: true,
    },
    Binding {
        mode: Page::Main,
        key: KeyCode::Char('?'),
        desc: "帮助",
        in_footer: true,
    },
    Binding {
        mode: Page::Main,
        key: KeyCode::Tab,
        desc: "切换面板",
        in_footer: false,
    },
    Binding {
        mode: Page::Main,
        key: KeyCode::Esc,
        desc: "回到节点列表",
        in_footer: false,
    },
    Binding {
        mode: Page::Main,
        key: KeyCode::Up,
        desc: "导航",
        in_footer: true,
    },
    Binding {
        mode: Page::Main,
        key: KeyCode::Down,
        desc: "导航",
        in_footer: true,
    },
    Binding {
        mode: Page::Main,
        key: KeyCode::PageUp,
        desc: "翻页",
        in_footer: false,
    },
    Binding {
        mode: Page::Main,
        key: KeyCode::PageDown,
        desc: "翻页",
        in_footer: false,
    },
    Binding {
        mode: Page::Main,
        key: KeyCode::Char('s'),
        desc: "开关mihomo",
        in_footer: false,
    },
    Binding {
        mode: Page::Main,
        key: KeyCode::Char('p'),
        desc: "系统代理",
        in_footer: false,
    },
    Binding {
        mode: Page::Main,
        key: KeyCode::Char('T'),
        desc: "TUN",
        in_footer: false,
    },
    Binding {
        mode: Page::Main,
        key: KeyCode::Char('c'),
        desc: "切换代理",
        in_footer: false,
    },
    Binding {
        mode: Page::Main,
        key: KeyCode::Char('t'),
        desc: "测速",
        in_footer: false,
    },
    Binding {
        mode: Page::Main,
        key: KeyCode::Char('r'),
        desc: "刷新节点",
        in_footer: false,
    },
    Binding {
        mode: Page::Main,
        key: KeyCode::Char('u'),
        desc: "添加订阅",
        in_footer: false,
    },
    Binding {
        mode: Page::Main,
        key: KeyCode::Char('l'),
        desc: "mihomo日志",
        in_footer: false,
    },
    Binding {
        mode: Page::Main,
        key: KeyCode::Char('e'),
        desc: "设置",
        in_footer: true,
    },
    Binding {
        mode: Page::Main,
        key: KeyCode::Enter,
        desc: "选中节点",
        in_footer: false,
    },
];

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

impl MainWindow {
    pub fn new() -> Self {
        Self {
            focus: Panel::Content,
            operation_log: OperationLog::new(),
            log_visible: 1,
        }
    }

    fn navigate(&mut self, app: &mut App, step: i32) {
        let len = app.state.nodes.len();
        if len == 0 {
            return;
        }
        app.state.select = wrap(app.state.select, len, step);
    }

    pub fn handle_key(&mut self, app: &mut App, key: KeyEvent) -> Option<Page> {
        match key.code {
            KeyCode::Char('q') => app.should_quit = true,
            KeyCode::Char('?') => return Some(Page::Help),
            KeyCode::Tab => self.focus = self.focus.cycle(),
            KeyCode::Esc => self.focus = Panel::Content,
            KeyCode::Up => match self.focus {
                Panel::OperationLog => self.operation_log.up(),
                Panel::Content => self.navigate(app, -1),
            },
            KeyCode::Down => match self.focus {
                Panel::OperationLog => {
                    let total = app.state.logs.len();
                    self.operation_log.down(total);
                }
                Panel::Content => self.navigate(app, 1),
            },
            KeyCode::PageUp => {
                if self.focus == Panel::OperationLog {
                    self.operation_log.page_up(self.log_visible);
                }
            }
            KeyCode::PageDown => {
                if self.focus == Panel::OperationLog {
                    let total = app.state.logs.len();
                    self.operation_log.page_down(total, self.log_visible);
                }
            }
            KeyCode::Char('s') => app.toggle_mihomo(),
            KeyCode::Char('p') => app.toggle_system_proxy(),
            KeyCode::Char('T') => app.toggle_tun(),
            KeyCode::Char('c') => return Some(Page::ProviderSelect),
            KeyCode::Char('t') => app.start_delay_test(),
            KeyCode::Char('r') => app.load_nodes(),
            KeyCode::Char('u') => return Some(Page::UrlInput),
            KeyCode::Char('l') => return Some(Page::MihomoLog),
            KeyCode::Char('e') => return Some(Page::Settings),
            KeyCode::Enter if self.focus == Panel::Content && !app.state.nodes.is_empty() => {
                let index = app.state.select;
                app.switch_node(index);
            }
            _ => {}
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
