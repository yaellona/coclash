//! 选择代理商窗口。
use crate::core::config::mihomo_config::MihomoConfig;
use crate::manager::Manager;
use crate::tui::Page;
use crate::tui::layout::popup_rect;
use crate::window;
use crossterm::event::KeyCode;
use ratatui::{
    Frame,
    style::{Color, Style},
    widgets::{Block, Borders, Clear, Paragraph},
};

pub struct ProviderSelectWindow {
    pub select: usize,
}

#[window(popup over Main)]
impl ProviderSelectWindow {
    pub fn new(manager: &Manager) -> Self {
        Self {
            select: initial_select(&manager.state.config),
        }
    }

    pub fn on_open(&mut self) {}

    fn provider_count(&self, manager: &Manager) -> usize {
        manager
            .state
            .config
            .proxy_providers
            .as_ref()
            .map(|p| p.len())
            .unwrap_or(0)
    }

    fn navigate(&mut self, manager: &Manager, step: i32) {
        let len = self.provider_count(manager);
        if len == 0 {
            return;
        }
        self.select = (self.select as i32 + step).rem_euclid(len as i32) as usize;
    }

    fn delete_current(&mut self, manager: &mut Manager) {
        let name = match manager.state.config.provider_key_by_index(self.select) {
            Some(n) => n,
            None => return,
        };
        if let Some(providers) = manager.state.config.proxy_providers.as_mut() {
            providers.shift_remove(&name);
        }
        match manager.state.config.write_to_path(&manager.config_path) {
            Ok(()) => manager.reload_config(),
            Err(e) => manager.log_err(e),
        }
    }

    #[key(KeyCode::Esc, "取消", footer = false)]
    fn cancel(&mut self, _manager: &mut Manager) -> Option<Page> {
        Some(Page::Main)
    }

    #[key(KeyCode::Up, "导航", footer = false)]
    fn up(&mut self, manager: &mut Manager) -> Option<Page> {
        self.navigate(manager, -1);
        None
    }

    #[key(KeyCode::Down, "导航", footer = false)]
    fn down(&mut self, manager: &mut Manager) -> Option<Page> {
        self.navigate(manager, 1);
        None
    }

    #[key(KeyCode::Char('d'), "删除代理", footer = false)]
    fn remove_provider(&mut self, manager: &mut Manager) -> Option<Page> {
        self.delete_current(manager);
        None
    }

    #[key(KeyCode::Enter, "确认", footer = false)]
    fn confirm(&mut self, manager: &mut Manager) -> Option<Page> {
        if let Some(name) = manager.state.config.provider_key_by_index(self.select) {
            manager.switch_provider(name);
        }
        Some(Page::Main)
    }

    pub fn draw(&mut self, manager: &mut Manager, f: &mut Frame) {
        let area = popup_rect(f.area());

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
        let items: Vec<String> = manager
            .state
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

/// 从配置推导初始选中的代理商
fn initial_select(config: &MihomoConfig) -> usize {
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
