use crate::app::App;
use crate::command::mihomo::MihomoStatus;
use ratatui::{
    Frame,
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    widgets::{Block, Borders, Cell, Clear, Paragraph, Row, Table, Wrap},
};

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let popup_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);

    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(popup_layout[1])[1]
}

pub fn render_url_input(f: &mut Frame, app: &App) {
    let area = centered_rect(60, 20, f.area());

    f.render_widget(Clear, area);

    let block = Block::default()
        .title("添加订阅 (Enter 确认, Esc 取消)")
        .borders(Borders::ALL)
        .style(Style::default().fg(Color::White));

    let inner = block.inner(area);
    f.render_widget(block, area);

    let input_layout = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(1)])
        .split(inner);

    let input_text = if app.url_input.is_empty() {
        "请输入订阅 URL...".to_string()
    } else {
        format!("{}▌", app.url_input)
    };

    let style = if app.url_input.is_empty() {
        Style::default().fg(Color::DarkGray)
    } else {
        Style::default().fg(Color::White)
    };

    let input = Paragraph::new(input_text)
        .style(style)
        .wrap(Wrap { trim: false });

    f.render_widget(input, input_layout[0]);
}

pub fn render_provider_select(f: &mut Frame, app: &App) {
    let area = centered_rect(60, 40, f.area());

    // 清除背景
    f.render_widget(Clear, area);

    let block = Block::default()
        .title("选择代理商")
        .title_bottom("(Enter 确认, Esc 取消, d 删除代理)")
        .borders(Borders::ALL)
        .style(Style::default().fg(Color::White));

    let inner = block.inner(area);
    f.render_widget(block, area);

    // 构建代理商列表
    let items: Vec<String> = app
        .config
        .proxy_providers
        .as_ref()
        .map(|providers| {
            providers
                .keys() // 获取所有 key
                .enumerate()
                .map(|(i, key)| {
                    let marker = if i == app.select_provider {
                        ">> "
                    } else {
                        "   "
                    };
                    format!("{}{}", marker, key)
                })
                .collect()
        })
        .unwrap_or_default();

    let list_text = items.join("\n");

    let style = Style::default().fg(Color::White);

    let list = Paragraph::new(list_text).style(style);

    f.render_widget(list, inner);
}

pub fn help_key(f: &mut Frame, app: &App) {
    let area = centered_rect(50, 40, f.area());
    f.render_widget(Clear, area);
    let rows: Vec<Row> = vec![
        Row::new(vec![
            Cell::from("q".to_string()),
            Cell::from(format!("退出")),
        ]),
        Row::new(vec![
            Cell::from("↑↓".to_string()),
            Cell::from(format!("导航")),
        ]),
        Row::new(vec![
            Cell::from("Enter".to_string()),
            Cell::from(format!("选中节点")),
        ]),
        Row::new(vec![
            Cell::from("r".to_string()),
            Cell::from(format!("刷新节点")),
        ]),
        Row::new(vec![
            Cell::from("u".to_string()),
            Cell::from(format!("添加订阅")),
        ]),
        Row::new(vec![
            Cell::from("l".to_string()),
            Cell::from(format!("查看mihomo日志")),
        ]),
        Row::new(vec![
            Cell::from("c".to_string()),
            Cell::from(format!("切换代理商")),
        ]),
        Row::new(vec![
            Cell::from("t".to_string()),
            Cell::from(format!("测速")),
        ]),
        Row::new(vec![
            Cell::from("p".to_string()),
            Cell::from(format!(
                "系统代理({})",
                if app.proxy_running {
                    "开启".to_string()
                } else {
                    "关闭".to_string()
                }
            )),
        ]),
        Row::new(vec![
            Cell::from("T".to_string()),
            Cell::from(format!(
                "TUN({})",
                if app.tun_enabled {
                    "开启".to_string()
                } else {
                    "关闭".to_string()
                }
            )),
        ]),
        Row::new(vec![
            Cell::from("s".to_string()),
            Cell::from(format!(
                "mihomo({})",
                match app.mihomo_status {
                    MihomoStatus::RunningByUs(_) => "开启".to_string(),
                    MihomoStatus::External => "外部运行".to_string(),
                    MihomoStatus::Stopped => "关闭".to_string(),
                }
            )),
        ]),
    ];

    let table = Table::new(rows, [Constraint::Length(10), Constraint::Min(0)]).block(
        Block::default()
            .title("帮助")
            .title_bottom("ESC退出")
            .borders(Borders::ALL),
    );

    f.render_widget(table, area);
}
