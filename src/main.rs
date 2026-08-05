mod app;
mod command;
mod config;
mod constants;
mod operation_log;
mod settings;
#[cfg(test)]
mod test;
use crossterm::{
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::io;

use command::mihomo::MihomoStatus;
use operation_log::LogType;

#[tokio::main]
async fn main() -> Result<(), io::Error> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    let (mut manager, mut windows) = app::Manager::new();
    match manager.mihomo.status {
        MihomoStatus::Stopped => {
            manager.start_mihomo();
        }
        _ => {
            manager
                .logs
                .add(LogType::Info, "检测到mihomo已在运行".to_string());
            manager.load_nodes();
        }
    }
    loop {
        terminal.draw(|f| windows.draw(&mut manager, f))?;
        if let Some(key) = app::event::poll_event(&manager.config.settings)? {
            windows.handle_key(&mut manager, key);
        }
        manager.poll(&mut windows);
        if manager.should_quit {
            break;
        }
    }
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}
