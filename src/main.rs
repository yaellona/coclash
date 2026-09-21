//! 入口：终端初始化 + 事件循环，业务逻辑见 `coclash` 库。
//! 心跳（Manager::start_heartbeat）是唯一的状态同步来源，主循环只消费重绘标志。
use coclash::core::mihomo::MihomoStatus;
use coclash::tui::PageId;
use coclash::tui::event::LoopEvent;
use coclash::{manager, tui};
use crossterm::{
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::io;

/// RAII：无论正常退出、错误还是 panic，都恢复终端
struct TerminalGuard;

impl TerminalGuard {
    fn enter() -> io::Result<Self> {
        enable_raw_mode()?;
        Ok(Self)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 先完成所有可能失败的 IO 初始化，再进入 raw mode / alternate screen
    let manager = manager::Manager::new()?;
    manager.start_heartbeat();
    if manager.state_lock().mihomo.status == MihomoStatus::Stopped {
        manager.start_mihomo();
    }

    let _guard = TerminalGuard::enter()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut pages = tui::Pages::new(&manager);

    // 条件重绘：仅在「有事件/状态变更」时构建帧，空闲时 CPU 趋近于 0。
    // 状态变更统一通过 Manager 的 redraw 标志置位（后台任务回灌 + 心跳同步），
    // 按键与终端缩放直接置 dirty；mihomo 日志页文件持续增长，每 tick 强制重绘。
    terminal.draw(|f| pages.draw(&manager, f))?;

    loop {
        let mut dirty = false;
        match tui::event::poll_event(manager.settings())? {
            LoopEvent::Key(key) => {
                pages.handle_key(&manager, key);
                dirty = true;
            }
            LoopEvent::Resize(_, _) => dirty = true,
            LoopEvent::Timeout => {}
        }
        dirty |= manager.take_redraw();
        if pages.current == PageId::MihomoLog {
            dirty = true;
        }
        if dirty {
            terminal.draw(|f| pages.draw(&manager, f))?;
        }
        if manager.quit_requested() {
            break;
        }
    }
    Ok(())
}
