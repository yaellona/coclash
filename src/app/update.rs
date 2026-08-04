use crate::app::msg::Msg;

use crate::operation_log::LogType;
use crossterm::event::KeyCode;

use super::PopupMode;
use super::actions;

impl super::App {
    pub fn update(&mut self, msg: Msg) {
        match msg {
            Msg::Key(k) => self.handle_key(k),
        }
    }

    pub fn poll(&mut self) {
        while let Ok(task) = self.async_rx.try_recv() {
            task(self);
        }
    }

    pub fn load_nodes(&self) {
        let tx = self.async_tx.clone();
        actions::reflash_nodes(tx, self.settings.clone());
    }

    fn handle_key(&mut self, key: KeyCode) {
        match self.popup_mode {
            PopupMode::UrlInput => {
                match key {
                    KeyCode::Esc => {
                        self.popup_mode = PopupMode::None;
                        self.url_input.clear();
                    }
                    KeyCode::Enter => self.submit_url(),
                    KeyCode::Backspace => {
                        self.url_input.pop();
                    }
                    KeyCode::Char(c) => {
                        self.url_input.push(c);
                    }
                    _ => {}
                }
                return;
            }
            PopupMode::AgencySelect => {
                match key {
                    KeyCode::Esc => {
                        self.popup_mode = PopupMode::None;
                    }
                    KeyCode::Up => self.navigate_provider(-1),
                    KeyCode::Down => self.navigate_provider(1),
                    KeyCode::Char('d') => self.delete_current_provider(),
                    KeyCode::Enter => {
                        if let Some(name) =
                            self.config.provider_key_by_index(self.select_provider)
                        {
                            self.switch_provider(name);
                        }
                    }
                    _ => {}
                }
                return;
            }
            PopupMode::HelpKey => match key {
                KeyCode::Esc => self.popup_mode = PopupMode::None,
                _ => {}
            },
            PopupMode::MihomoLog => match key {
                KeyCode::Esc => self.popup_mode = PopupMode::None,
                KeyCode::Up => self.mihomo_log.scroll_up(),
                KeyCode::Down => self.mihomo_log.scroll_down(),
                KeyCode::PageUp => self.mihomo_log.page_up(),
                KeyCode::PageDown => self.mihomo_log.page_down(),
                _ => {}
            },
            PopupMode::None => match key {
                KeyCode::Char('q') => {
                    self.should_quit = true;
                }

                KeyCode::Char('s') => {
                    self.toggle_mihomo();
                }

                KeyCode::Char('p') => {
                    self.toggle_system_proxy();
                }
                KeyCode::Char('T') => {
                    self.toggle_tun();
                }
                KeyCode::Char('c') => self.popup_mode = PopupMode::AgencySelect,

                KeyCode::Char('t') => self.start_delay_test(),
                KeyCode::Char('r') => {
                    let tx = self.async_tx.clone();
                    actions::reflash_nodes(tx, self.settings.clone());
                }
                KeyCode::Char('u') => self.popup_mode = PopupMode::UrlInput,
                KeyCode::Char('l') => self.popup_mode = PopupMode::MihomoLog,
                KeyCode::Up => self.navigate_node(-1),
                KeyCode::Down => self.navigate_node(1),
                KeyCode::Enter => {
                    if !self.current_nodes.is_empty() {
                        self.active_node = Some(self.select_node);
                        let node_name = self.current_nodes[self.select_node].name.clone();
                        let tx = self.async_tx.clone();
                        actions::switch_node(tx, self.settings.clone(), node_name);
                    }
                }
                KeyCode::Char('?') => {
                    self.popup_mode = PopupMode::HelpKey;
                }
                _ => {}
            },
        }
    }

    fn switch_provider(&mut self, name: String) {
        match self.config.prepare_switch_provider(&name, &self.config_path) {
            Ok(()) => {
                self.logs
                    .add_log(LogType::Info, "正在切换代理商...".to_string());
                let tx = self.async_tx.clone();
                actions::reload(tx, self.settings.clone(), self.config_path.clone());
            }
            Err(e) => self.logs.add_log(LogType::Error, e.to_string()),
        }
    }
}
