//! 事件轮询：返回完整的 `KeyEvent`（保留修饰键），超时驱动主循环节拍。
use crossterm::event::{self, Event, KeyEvent, KeyEventKind};
use std::io;

use crate::settings::Settings;

pub fn poll_event(settings: &Settings) -> io::Result<Option<KeyEvent>> {
    if event::poll(settings.poll_interval())?
        && let Event::Key(key) = event::read()?
        && key.kind == KeyEventKind::Press
    {
        return Ok(Some(key));
    }
    Ok(None)
}
