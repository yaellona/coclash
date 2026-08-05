use crate::app::keymap;
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
        if let Some(binding) = keymap::lookup(self.popup_mode, key) {
            (binding.run)(self);
            return;
        }
        // 兜底：URL 输入模式下其余字符直接输入
        if matches!(self.popup_mode, PopupMode::UrlInput)
            && let KeyCode::Char(c) = key
        {
            self.url_input.push(c);
        }
    }

    pub(crate) fn switch_provider(&mut self, name: String) {
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
