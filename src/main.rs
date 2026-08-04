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

use app::msg::Msg;
use command::mihomo::MihomoStatus;
use operation_log::{LogType};

#[tokio::main]
async fn main() -> Result<(), io::Error> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    let mut app = app::App::new();
    match app.mihomo_status {
        MihomoStatus::Stopped => {
            app.start_mihomo();
        }
        _ => {
            app.logs
                .add_log(LogType::Info, "检测到mihomo已在运行".to_string());
            app.load_nodes();
        }
    }
    loop {
        terminal.draw(|f| app.draw(f))?;
        if let Some(key) = app::event::poll_event(&app.settings)? {
            app.update(Msg::Key(key));
        }
        app.poll();
        if app.should_quit {
            break;
        }
    }
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}
