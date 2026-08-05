use crate::app::actions::{reflash_nodes, switch_node};
use crate::app::keymap::{Binding, footer_text, keymap};
use crate::app::{App, Focus, PopupMode};
use crate::app::ui::components::{content, footer, operation_log, running_info};
use crossterm::event::KeyCode;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout},
};
use std::sync::LazyLock;

/// 主页面按键：本页生效的按键都在这里注册（`#[keymap]` 自动生成 `MAIN_BINDINGS`）
#[keymap(name = "MAIN_BINDINGS")]
impl App {
    #[key('q', desc = "退出", footer)]
    fn key_quit(&mut self) {
        self.should_quit = true;
    }

    #[key('?', desc = "帮助", footer)]
    fn key_open_help(&mut self) {
        self.popup_mode = PopupMode::HelpKey;
        self.help_state.select(Some(0));
    }

    #[key(KeyCode::Tab, desc = "切换面板")]
    fn key_switch_focus(&mut self) {
        self.focus = match self.focus {
            Focus::Nodes => Focus::Log,
            Focus::Log => Focus::Nodes,
        };
    }

    #[key(KeyCode::Esc, desc = "回到节点列表")]
    fn key_reset_focus(&mut self) {
        self.focus = Focus::Nodes;
    }

    #[key(KeyCode::Up, desc = "导航", footer)]
    fn key_node_up(&mut self) {
        match self.focus {
            Focus::Nodes => self.navigate_node(-1),
            Focus::Log => self.log_scroll_up(),
        }
    }

    #[key(KeyCode::Down, desc = "导航", footer)]
    fn key_node_down(&mut self) {
        match self.focus {
            Focus::Nodes => self.navigate_node(1),
            Focus::Log => self.log_scroll_down(),
        }
    }

    #[key(KeyCode::PageUp, desc = "翻页")]
    fn key_page_up(&mut self) {
        if self.focus == Focus::Log {
            self.log_page_up();
        }
    }

    #[key(KeyCode::PageDown, desc = "翻页")]
    fn key_page_down(&mut self) {
        if self.focus == Focus::Log {
            self.log_page_down();
        }
    }

    #[key('s', desc = "开关mihomo")]
    fn key_toggle_mihomo(&mut self) {
        self.toggle_mihomo();
    }

    #[key('p', desc = "系统代理")]
    fn key_toggle_proxy(&mut self) {
        self.toggle_system_proxy();
    }

    #[key('T', desc = "TUN")]
    fn key_toggle_tun(&mut self) {
        self.toggle_tun();
    }

    #[key('c', desc = "切换代理")]
    fn key_open_agency_select(&mut self) {
        self.popup_mode = PopupMode::AgencySelect;
    }

    #[key('t', desc = "测速")]
    fn key_delay_test(&mut self) {
        self.start_delay_test();
    }

    #[key('r', desc = "刷新节点")]
    fn key_refresh_nodes(&mut self) {
        let tx = self.async_tx.clone();
        reflash_nodes(tx, self.settings.clone());
    }

    #[key('u', desc = "添加订阅")]
    fn key_open_url_input(&mut self) {
        self.popup_mode = PopupMode::UrlInput;
    }

    #[key('l', desc = "mihomo日志")]
    fn key_open_mihomo_log(&mut self) {
        self.popup_mode = PopupMode::MihomoLog;
    }

    #[key(KeyCode::Enter, desc = "选中节点")]
    fn key_select_node(&mut self) {
        if self.focus != Focus::Nodes {
            return;
        }
        if !self.current_nodes.is_empty() {
            self.active_node = Some(self.select_node);
            let name = self.current_nodes[self.select_node].name.clone();
            let tx = self.async_tx.clone();
            switch_node(tx, self.settings.clone(), name);
        }
    }
}

/// 主页面绘制：底部栏 + 左节点列表 + 右状态信息/操作日志
pub fn draw(f: &mut Frame, app: &mut App) {
    let size = f.area();
    let footer_text = footer_text();
    let focus = app.focus;

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

    let content = content::render(&app.current_nodes, focus == Focus::Nodes);
    f.render_widget(footer, main_chunks[1]);
    if constraint.len() > 1 {
        let chunks3 = Layout::default()
            .direction(Direction::Vertical)
            .constraints([Constraint::Length(7), Constraint::Min(0)])
            .split(chunks2[1]);
        let info = running_info::render(app);
        f.render_widget(info, chunks3[0]);
        let log = operation_log::render(
            &app.logs,
            chunks2[1].width as usize - 10,
            focus == Focus::Log,
        );
        if !app.logs.is_empty() {
            let max = app.logs.len() - 1;
            // 跟随模式下始终贴底；否则仅在日志缩短导致选中越界时钳制
            if app.log_follow || app.log_state.selected().is_some_and(|s| s > max) {
                app.log_state.select(Some(max));
            }
        }
        f.render_stateful_widget(log, chunks3[1], &mut app.log_state);
    }

    f.render_stateful_widget(
        &content,
        chunks2[0],
        &mut ratatui::widgets::TableState::default().with_selected(Some(app.select_node)),
    );
}
