//! 状态信息组件（只读展示）：优先显示心跳同步的运行时信息，停止时回落本地配置。
use crate::core::mihomo::MihomoStatus;
use crate::manager::state::AppState;
use crate::tui::layout::format_bytes;
use ratatui::{
    layout::Constraint,
    style::Color,
    widgets::{Block, Borders, Cell, Row, Table},
};

pub struct RunningInfo;

/// 运行模式展示：runtime 优先（`rule` → `Rule`），否则本地配置
fn mode_text(state: &AppState) -> String {
    match state.mihomo.runtime.mode.as_deref() {
        Some(m) if !m.is_empty() => {
            let mut chars = m.chars();
            match chars.next() {
                Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
                None => m.to_string(),
            }
        }
        _ => state.config.mode.clone(),
    }
}

fn port_text(state: &AppState) -> String {
    match state.proxy_addr() {
        Some(addr) => addr
            .rsplit_once(':')
            .map(|(_, p)| p.to_string())
            .unwrap_or(addr),
        None => "-".to_string(),
    }
}

impl RunningInfo {
    pub fn render(state: &AppState) -> Table<'_> {
        let mihomo = &state.mihomo;
        let tun_on = mihomo
            .runtime
            .tun_enabled
            .unwrap_or_else(|| state.tun_enabled());
        let traffic = if mihomo.runtime.api_ready {
            format!(
                "↑{} ↓{}",
                format_bytes(mihomo.runtime.upload_total),
                format_bytes(mihomo.runtime.download_total)
            )
        } else {
            "-".to_string()
        };

        let rows: Vec<Row> = vec![
            Row::new(vec![
                Cell::from("代理".to_string()),
                Cell::from(state.proxy_addr().unwrap_or_else(|| "-".to_string()))
                    .style(Color::LightMagenta),
            ]),
            Row::new(vec![
                Cell::from("节点".to_string()),
                match mihomo.active_node.and_then(|i| mihomo.nodes.get(i)) {
                    Some(node) => Cell::from(node.name.to_string()).style(Color::LightGreen),
                    None => Cell::from("无".to_string()).style(Color::LightYellow),
                },
            ]),
            Row::new(vec![
                Cell::from("mihomo内核".to_string()),
                match mihomo.status {
                    MihomoStatus::Running(pid) if pid != 0 => Cell::from(format!(
                        "{} (PID {pid})",
                        mihomo.runtime.version.as_deref().unwrap_or("运行中")
                    ))
                    .style(Color::LightGreen),
                    MihomoStatus::Running(_) => Cell::from(format!(
                        "{} (运行中)",
                        mihomo.runtime.version.as_deref().unwrap_or("运行中")
                    ))
                    .style(Color::LightGreen),
                    MihomoStatus::Stopped => Cell::from("已停止").style(Color::LightYellow),
                },
            ]),
            Row::new(vec![
                Cell::from("模式/端口".to_string()),
                Cell::from(format!("{} :{}", mode_text(state), port_text(state)))
                    .style(Color::LightCyan),
            ]),
            Row::new(vec![
                Cell::from("代理/TUN".to_string()),
                Cell::from(format!(
                    "系统代理 {} | TUN {}",
                    on_off(mihomo.proxy_running),
                    on_off(tun_on)
                ))
                .style(if mihomo.proxy_running || tun_on {
                    Color::LightGreen
                } else {
                    Color::LightYellow
                }),
            ]),
            Row::new(vec![
                Cell::from("流量(累计)".to_string()),
                Cell::from(traffic).style(Color::LightBlue),
            ]),
        ];

        Table::new(rows, [Constraint::Length(20), Constraint::Min(0)])
            .block(Block::default().title("状态信息").borders(Borders::ALL))
    }
}

fn on_off(v: bool) -> &'static str {
    if v { "开" } else { "关" }
}
