use crate::app::Manager;
use crate::app::keymap::{Binding, popup};
use crate::app::tasks;
use crate::app::ui::{Popup, Window, WindowCtx};
use crate::app::ui::pages::centered_rect;
use crate::app::ui::pages::main::MAIN;
use crate::app::WindowId;
use crossterm::event::KeyCode;
use ratatui::{
    Frame,
    style::{Color, Style},
    widgets::{Block, Borders, Clear, Paragraph},
};
use std::sync::LazyLock;

/// 选择代理商窗口：当前选中项状态
pub struct ProviderSelectWindow {
    pub select: usize,
}

impl ProviderSelectWindow {
    pub(crate) fn new(ctx: &WindowCtx) -> Self {
        Self {
            select: initial_select(ctx.config),
        }
    }

    fn navigate(&mut self, m: &Manager, step: i32) {
        let len = m
            .config
            .config
            .proxy_providers
            .as_ref()
            .map(|p| p.len())
            .unwrap_or(0);
        if len == 0 {
            return;
        }
        self.select = (self.select as i32 + step).rem_euclid(len as i32) as usize;
    }

    fn delete_current(&mut self, m: &mut Manager) {
        let name = match m.config.config.provider_key_by_index(self.select) {
            Some(n) => n,
            None => return,
        };
        if let Some(providers) = m.config.config.proxy_providers.as_mut() {
            providers.shift_remove(&name);
        }
        let _ = m.config.config.write_to_path(&m.config.config_path);
        tasks::reload(
            m.tasks.tx.clone(),
            m.config.settings.clone(),
            m.config.config_path.clone(),
        );
    }

    fn confirm(&mut self, m: &mut Manager) {
        if let Some(name) = m.config.config.provider_key_by_index(self.select) {
            m.switch_provider(name);
        }
    }
}

/// 从配置推导初始选中的代理商
fn initial_select(config: &crate::config::mihomo_config::MihomoConfig) -> usize {
    let mut select = 0;
    if !config.proxy_groups.is_empty()
        && !config.proxy_groups[0].use_list.is_empty()
        && let Some(idx) = config
            .proxy_groups
            .first()
            .and_then(|g| g.use_list.first())
            .and_then(|name| config.provider_index_by_key(name))
    {
        select = idx;
    }
    select
}

#[popup(name = "provider_select")]
impl ProviderSelectWindow {
    #[key(KeyCode::Esc, desc = "取消")]
    fn key_close(&mut self, m: &mut Manager) {
        m.current_window = MAIN;
    }

    #[key(KeyCode::Up, desc = "导航")]
    fn key_provider_up(&mut self, m: &mut Manager) {
        self.navigate(m, -1);
    }

    #[key(KeyCode::Down, desc = "导航")]
    fn key_provider_down(&mut self, m: &mut Manager) {
        self.navigate(m, 1);
    }

    #[key('d', desc = "删除代理")]
    fn key_delete_provider(&mut self, m: &mut Manager) {
        self.delete_current(m);
    }

    #[key(KeyCode::Enter, desc = "确认")]
    fn key_confirm(&mut self, m: &mut Manager) {
        self.confirm(m);
    }

    #[render]
    fn draw(&mut self, m: &mut Manager, f: &mut Frame) {
        let area = centered_rect(60, 40, f.area());

        // 清除背景
        f.render_widget(Clear, area);

        let block = Block::default()
            .title("选择代理商")
            .title_bottom("(Enter 确认, Esc 取消, d 删除代理)")
            .borders(Borders::ALL)
            .style(Style::default().fg(Color::White));

        let inner = block.inner(area);
        f.render_widget(block, area);

        // 构建代理商列表
        let items: Vec<String> = m
            .config
            .config
            .proxy_providers
            .as_ref()
            .map(|providers| {
                providers
                    .keys() // 获取所有 key
                    .enumerate()
                    .map(|(i, key)| {
                        let marker = if i == self.select { ">> " } else { "   " };
                        format!("{}{}", marker, key)
                    })
                    .collect()
            })
            .unwrap_or_default();

        let list_text = items.join("\n");

        let style = Style::default().fg(Color::White);

        let list = Paragraph::new(list_text).style(style);

        f.render_widget(list, inner);
    }
}
