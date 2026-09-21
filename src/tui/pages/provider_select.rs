//! 选择订阅窗口。
use crate::core::config::mihomo_config::MihomoConfig;
use crate::manager::commands::Effect;
use crate::manager::state::AppState;
use crate::tui::action::{Binding, KeyPattern};
use crate::tui::cmd::{Cmd, Nav};
use crate::tui::layout::{popup_rect, wrap_index};
use crate::tui::page::Page;
use crate::tui::pages::PageId;
use crossterm::event::KeyCode;
use ratatui::{
    Frame,
    style::{Color, Style},
    widgets::{Block, Borders, Clear, Paragraph},
};

pub struct ProviderSelectPage {
    pub select: usize,
}

impl ProviderSelectPage {
    pub fn new(state: &AppState) -> Self {
        Self {
            select: initial_select(&state.config),
        }
    }

    fn provider_count(state: &AppState) -> usize {
        state
            .config
            .proxy_providers
            .as_ref()
            .map(|p| p.len())
            .unwrap_or(0)
    }

    fn navigate(&mut self, state: &AppState, step: i32) {
        let len = Self::provider_count(state);
        if len == 0 {
            return;
        }
        self.select = wrap_index(self.select, len, step);
    }

    fn cancel(&mut self, _: &AppState) -> Cmd {
        Cmd::back()
    }

    fn up(&mut self, state: &AppState) -> Cmd {
        self.navigate(state, -1);
        Cmd::none()
    }

    fn down(&mut self, state: &AppState) -> Cmd {
        self.navigate(state, 1);
        Cmd::none()
    }

    fn remove_provider(&mut self, state: &AppState) -> Cmd {
        let len = Self::provider_count(state);
        let Some(name) = state.config.provider_key_by_index(self.select) else {
            return Cmd::none();
        };
        // 删除后的越界收敛（删除动作由命令层执行，这里按删除前的长度计算）
        if len > 0 {
            self.select = self.select.min(len - 1);
        }
        Cmd::effect(Effect::DeleteProvider(name))
    }

    fn confirm(&mut self, state: &AppState) -> Cmd {
        match state.config.provider_key_by_index(self.select) {
            Some(name) => Cmd::new(Nav::Back, Effect::SwitchProvider(name)),
            None => Cmd::back(),
        }
    }
}

impl Page for ProviderSelectPage {
    const BINDINGS: &'static [Binding<Self>] = &[
        Binding::on(KeyPattern::Code(KeyCode::Esc), "取消", false, Self::cancel),
        Binding::on(KeyPattern::Code(KeyCode::Up), "导航", false, Self::up),
        Binding::on(KeyPattern::Code(KeyCode::Down), "导航", false, Self::down),
        Binding::on(
            KeyPattern::Code(KeyCode::Char('d')),
            "删除订阅",
            false,
            Self::remove_provider,
        ),
        Binding::on(
            KeyPattern::Code(KeyCode::Enter),
            "确认",
            false,
            Self::confirm,
        ),
    ];

    fn parent(&self) -> Option<PageId> {
        Some(PageId::Main)
    }

    fn draw(&mut self, state: &AppState, f: &mut Frame) {
        let area = popup_rect(f.area());

        // 清除背景
        f.render_widget(Clear, area);

        let block = Block::default()
            .title("选择订阅")
            .title_bottom("(Enter 确认, Esc 取消, d 删除订阅)")
            .borders(Borders::ALL)
            .style(Style::default().fg(Color::White));

        let inner = block.inner(area);
        f.render_widget(block, area);

        // 构建订阅列表
        let items: Vec<String> = state
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

/// 从配置推导初始选中的订阅
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

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyEvent, KeyModifiers};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn state_with_providers() -> AppState {
        let mut state = AppState::test_fixture();
        state
            .config
            .insert_sub("https://example.com/sub".into(), "订阅1".into());
        state
            .config
            .insert_sub("https://example.com/sub2".into(), "订阅2".into());
        state
    }

    #[test]
    fn test_confirm_switches_and_returns() {
        let state = state_with_providers();
        let mut page = ProviderSelectPage::new(&state);
        let cmd = page.handle_key(&state, key(KeyCode::Enter));
        assert_eq!(
            cmd,
            Cmd::new(Nav::Back, Effect::SwitchProvider("订阅1".into()))
        );
    }

    #[test]
    fn test_delete_provider() {
        let state = state_with_providers();
        let mut page = ProviderSelectPage::new(&state);
        let cmd = page.handle_key(&state, key(KeyCode::Char('d')));
        assert_eq!(cmd, Cmd::effect(Effect::DeleteProvider("订阅1".into())));
    }

    #[test]
    fn test_empty_is_noop_and_backs() {
        let state = AppState::test_fixture();
        let mut page = ProviderSelectPage::new(&state);
        assert_eq!(page.handle_key(&state, key(KeyCode::Enter)), Cmd::back());
        assert_eq!(page.handle_key(&state, key(KeyCode::Esc)), Cmd::back());
        assert_eq!(
            page.handle_key(&state, key(KeyCode::Char('d'))),
            Cmd::none()
        );
    }
}
