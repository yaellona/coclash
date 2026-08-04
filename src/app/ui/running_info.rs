use crate::app::App;
use crate::command::mihomo::MihomoStatus;
use ratatui::{
    layout::Constraint,
    style::Color,
    widgets::{Block, Borders, Cell, Row, Table},
};
pub fn render(app: &App) -> Table<'_> {
    let rows: Vec<Row> = vec![
        Row::new(vec![
            Cell::from("代理".to_string()),
            Cell::from(format!("127.0.0.1:{}", app.settings.mixed_port)).style(Color::LightMagenta),
        ]),
        Row::new(vec![
            Cell::from("节点".to_string()),
            match app.active_node.and_then(|i| app.current_nodes.get(i)) {
                Some(node) => Cell::from(node.name.to_string()).style(Color::LightGreen),
                None => Cell::from("无".to_string()).style(Color::LightYellow),
            },
        ]),
        Row::new(vec![
            Cell::from("mihomo内核".to_string()),
            match app.mihomo_status {
                MihomoStatus::RunningByUs(pid) => {
                    Cell::from(format!("运行中 (PID {pid})")).style(Color::LightGreen)
                }
                MihomoStatus::External => Cell::from("运行中 (外部)").style(Color::LightGreen),
                MihomoStatus::Stopped => Cell::from("已停止").style(Color::LightYellow),
            },
        ]),
        Row::new(vec![
            Cell::from("系统代理".to_string()),
            if app.proxy_running {
                Cell::from("开启".to_string()).style(Color::LightGreen)
            } else {
                Cell::from("关闭".to_string()).style(Color::LightYellow)
            },
        ]),
        Row::new(vec![
            Cell::from("TUN".to_string()),
            if app.tun_enabled {
                Cell::from("开启".to_string()).style(Color::LightGreen)
            } else {
                Cell::from("关闭".to_string()).style(Color::LightYellow)
            },
        ]),
    ];

    Table::new(rows, [Constraint::Length(20), Constraint::Min(0)])
        .block(Block::default().title("状态信息").borders(Borders::ALL))
}
