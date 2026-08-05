pub mod components;
pub mod pages;

use crate::app::PopupMode;
use ratatui::Frame;

use crate::app::App;
impl App {
    /// 主界面常驻绘制，弹窗作为覆盖层叠加其上
    pub fn draw(&mut self, f: &mut Frame) {
        pages::main::draw(f, self);
        match self.popup_mode {
            PopupMode::UrlInput => pages::url_input::draw(f, self),
            PopupMode::AgencySelect => pages::provider_select::draw(f, self),
            PopupMode::HelpKey => pages::help::draw(f, self),
            PopupMode::MihomoLog => pages::mihomo_log::draw(f, self),
            PopupMode::None => {}
        }
    }
}
