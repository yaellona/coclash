//! 入口：终端初始化 + 事件循环，业务逻辑见 `coclash` 库。
use coclash::core::mihomo::MihomoStatus;
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
    let status = manager.state_lock().mihomo_status;
    match status {
        MihomoStatus::Stopped => manager.start_mihomo(),
        _ => {
            manager.log("检测到mihomo已在运行");
            manager.load_nodes();
        }
    }

    let _guard = TerminalGuard::enter()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut windows = tui::Windows::new(&manager);

    loop {
        terminal.draw(|f| windows.draw(&manager, f))?;
        if let Some(key) = tui::event::poll_event(&manager.settings)? {
            windows.handle_key(&manager, key);
        }
        if manager.should_quit.load(std::sync::atomic::Ordering::Relaxed) {
            break;
        }
    }
    Ok(())
}
