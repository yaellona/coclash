mod app;
mod command;
mod config;
mod constants;
mod error;
mod operation_log;
mod settings;
#[cfg(test)]
mod test;
mod ui;

use app::App;
use command::mihomo::MihomoStatus;
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
    let mut app = App::new()?;
    match app.state.mihomo_status {
        MihomoStatus::Stopped => app.start_mihomo(),
        _ => {
            app.log("检测到mihomo已在运行");
            app.load_nodes();
        }
    }

    let _guard = TerminalGuard::enter()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut windows = ui::Windows::new(&app);

    loop {
        terminal.draw(|f| windows.draw(&mut app, f))?;
        if let Some(key) = app::event::poll_event(&app.settings)? {
            windows.handle_key(&mut app, key);
        }
        app.drain_tasks();
        if app.should_quit {
            break;
        }
    }
    Ok(())
}
