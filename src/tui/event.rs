//! 事件轮询：返回按键/缩放事件，超时（无输入）驱动主循环节拍。
//! 缩放必须透传——条件重绘后，终端尺寸变化需要触发重绘。
use crossterm::event::{self, Event, KeyEvent, KeyEventKind};
use std::io;

use crate::settings::Settings;

/// 主循环事件：按键即时处理；缩放触发重绘；超时检查是否需要重绘。
#[derive(Debug)]
pub enum LoopEvent {
    Key(KeyEvent),
    Resize(u16, u16),
    Timeout,
}

pub fn poll_event(settings: &Settings) -> io::Result<LoopEvent> {
    if event::poll(settings.poll_interval())? {
        match event::read()? {
            Event::Key(key) if key.kind == KeyEventKind::Press => Ok(LoopEvent::Key(key)),
            Event::Resize(w, h) => Ok(LoopEvent::Resize(w, h)),
            _ => Ok(LoopEvent::Timeout),
        }
    } else {
        Ok(LoopEvent::Timeout)
    }
}
