//! 入口：CLI 分派（默认 TUI，`core` 直通内嵌 mihomo）+ 终端初始化 + 事件循环。
//! 心跳（Manager::start_heartbeat）是唯一的状态同步来源，主循环只消费重绘标志。
use coclash::cli::{self, Cli};
use coclash::core::mihomo::MihomoStatus;
use coclash::tui::PageId;
use coclash::tui::event::LoopEvent;
use coclash::{manager, tui};
use crossterm::{
    cursor::Show,
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::io;

/// RAII：无论正常退出、错误还是 panic，都恢复终端（含光标的显示状态）
struct TerminalGuard;

impl TerminalGuard {
    fn enter() -> io::Result<Self> {
        // panic 默认在 alternate screen 里打印，随后被清屏吞掉：
        // 先恢复终端再交给默认 hook，panic 信息才会留在用户终端上
        let default_hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let _ = disable_raw_mode();
            let _ = execute!(io::stdout(), Show, LeaveAlternateScreen);
            default_hook(info);
        }));
        enable_raw_mode()?;
        Ok(Self)
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
        // ratatui 每帧都会隐藏光标，退出时必须显式恢复
        let _ = execute!(io::stdout(), Show, LeaveAlternateScreen);
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    match Cli::parse(std::env::args_os().skip(1)) {
        Cli::Help => {
            print!("{}", cli::help_text());
            Ok(())
        }
        // Unix 成功时进程已被 mihomo 替换（不返回）；Windows 等待结束并透传退出码
        Cli::Core(args) => Ok(coclash::core::mihomo::exec_core(&args)?),
        Cli::Tui => run_tui(),
    }
}

/// TUI 模式：手建 tokio runtime（core 模式不初始化任何运行时）
fn run_tui() -> Result<(), Box<dyn std::error::Error>> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()?
        .block_on(tui_loop())
}

async fn tui_loop() -> Result<(), Box<dyn std::error::Error>> {
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
