//! 设置窗口：字段视图 + 规则子视图。
use crate::app::App;
use crate::config::mihomo_config::{Dns, Tun};
use crate::ui::Page;
use crate::ui::keymap::Binding;
use crate::ui::layout::{display_width, popup_rect};
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    Frame,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

pub const BINDINGS: &[Binding] = &[
    Binding {
        mode: Page::Settings,
        key: KeyCode::Esc,
        desc: "保存并关闭",
        in_footer: false,
    },
    Binding {
        mode: Page::Settings,
        key: KeyCode::Enter,
        desc: "编辑/切换",
        in_footer: false,
    },
    Binding {
        mode: Page::Settings,
        key: KeyCode::Up,
        desc: "导航",
        in_footer: false,
    },
    Binding {
        mode: Page::Settings,
        key: KeyCode::Down,
        desc: "导航",
        in_footer: false,
    },
    Binding {
        mode: Page::Settings,
        key: KeyCode::Char('r'),
        desc: "规则",
        in_footer: false,
    },
    Binding {
        mode: Page::Settings,
        key: KeyCode::Char('a'),
        desc: "添加规则",
        in_footer: false,
    },
    Binding {
        mode: Page::Settings,
        key: KeyCode::Char('d'),
        desc: "删除规则",
        in_footer: false,
    },
];

/// 编辑缓冲：字段/规则编辑态共用（`rule: None` 表示新增规则）
struct EditState {
    rule: Option<usize>,
    buffer: String,
}

impl EditState {
    fn field(buffer: String) -> Self {
        Self { rule: None, buffer }
    }

    fn rule(rule: Option<usize>, buffer: String) -> Self {
        Self { rule, buffer }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum View {
    Fields,
    Rules,
}

/// 设置窗口：字段视图 + 规则子视图
pub struct SettingsWindow {
    view: View,
    fields_select: usize,
    rules_select: usize,
    rules_scroll: usize,
    editing: Option<EditState>,
    /// 是否有未落盘的改动（关闭时仅在有改动时写盘 + 重载）
    changed: bool,
}

impl SettingsWindow {
    pub fn new() -> Self {
        Self {
            view: View::Fields,
            fields_select: 0,
            rules_select: 0,
            rules_scroll: 0,
            editing: None,
            changed: false,
        }
    }

    pub fn on_open(&mut self) {
        self.view = View::Fields;
        self.fields_select = 0;
        self.rules_select = 0;
        self.rules_scroll = 0;
        self.editing = None;
        self.changed = false;
    }

    /// 关闭：仅当有改动时写盘 + 重载 mihomo
    fn close(&mut self, app: &mut App) {
        if self.changed {
            match app.state.config.write_to_path(&app.config_path) {
                Ok(()) => app.reload_config(),
                Err(e) => app.log_err(e),
            }
        }
    }

    fn toggle_field(&mut self, app: &mut App) {
        let i = self.fields_select;
        if i >= FIELD_COUNT {
            return;
        }
        let kind = FIELDS[i].kind;
        match kind {
            FieldKind::Mode => {
                app.state.config.mode = cycle(&MODES, &app.state.config.mode).to_string();
                self.changed = true;
            }
            FieldKind::LogLevel => {
                app.state.config.log_level =
                    cycle(&LOG_LEVELS, &app.state.config.log_level).to_string();
                self.changed = true;
            }
            FieldKind::AllowLan => {
                app.state.config.allow_lan = !app.state.config.allow_lan;
                self.changed = true;
            }
            FieldKind::UnifiedDelay => {
                app.state.config.unified_delay = !app.state.config.unified_delay;
                self.changed = true;
            }
            FieldKind::Tun => {
                let on = !app.state.config.tun.as_ref().is_some_and(|t| t.enable);
                if on {
                    let tun = app
                        .state
                        .config
                        .tun
                        .get_or_insert_with(Tun::default_enabled);
                    tun.enable = true;
                    app.state
                        .config
                        .dns
                        .get_or_insert_with(Dns::default_enabled)
                        .enable = true;
                } else if let Some(t) = app.state.config.tun.as_mut() {
                    t.enable = false;
                }
                self.changed = true;
            }
            FieldKind::Dns => {
                let on = !app.state.config.dns.as_ref().is_some_and(|d| d.enable);
                if on {
                    app.state
                        .config
                        .dns
                        .get_or_insert_with(Dns::default_enabled)
                        .enable = true;
                } else if let Some(d) = app.state.config.dns.as_mut() {
                    d.enable = false;
                }
                self.changed = true;
            }
            FieldKind::Port | FieldKind::SocksPort | FieldKind::KeepAlive => {
                let buffer = self.field_value(app, kind);
                self.editing = Some(EditState::field(buffer));
            }
            FieldKind::Rules => self.view = View::Rules,
        }
    }

    fn field_value(&self, app: &App, kind: FieldKind) -> String {
        match kind {
            FieldKind::Mode => app.state.config.mode.clone(),
            FieldKind::Port => app.state.config.port.to_string(),
            FieldKind::SocksPort => app.state.config.socks_port.to_string(),
            FieldKind::AllowLan => on_off(app.state.config.allow_lan),
            FieldKind::LogLevel => app.state.config.log_level.clone(),
            FieldKind::UnifiedDelay => on_off(app.state.config.unified_delay),
            FieldKind::KeepAlive => app.state.config.keep_alive_interval.to_string(),
            FieldKind::Tun => on_off(app.state.config.tun.as_ref().is_some_and(|t| t.enable)),
            FieldKind::Dns => on_off(app.state.config.dns.as_ref().is_some_and(|d| d.enable)),
            FieldKind::Rules => format!("{} 条", app.state.config.rules.len()),
        }
    }

    /// 确认编辑：解析成功才写回配置
    fn apply_edit(&mut self, app: &mut App) {
        let Some(edit) = self.editing.take() else {
            return;
        };
        if let Some(rule_idx) = edit.rule {
            if !edit.buffer.is_empty() {
                let rules = &mut app.state.config.rules;
                if rule_idx < rules.len() {
                    rules[rule_idx] = edit.buffer.clone();
                    self.changed = true;
                }
            }
            return;
        }
        match self.view {
            View::Rules => {
                // 新增规则
                if !edit.buffer.is_empty() {
                    app.state.config.rules.push(edit.buffer.clone());
                    self.changed = true;
                }
            }
            View::Fields => match FIELDS[self.fields_select].kind {
                FieldKind::Port => {
                    if let Ok(v) = edit.buffer.parse::<u16>() {
                        app.state.config.port = v;
                        self.changed = true;
                    }
                }
                FieldKind::SocksPort => {
                    if let Ok(v) = edit.buffer.parse::<u16>() {
                        app.state.config.socks_port = v;
                        self.changed = true;
                    }
                }
                FieldKind::KeepAlive => {
                    if let Ok(v) = edit.buffer.parse::<u32>() {
                        app.state.config.keep_alive_interval = v;
                        self.changed = true;
                    }
                }
                _ => {}
            },
        }
    }

    fn rules_count(app: &App) -> usize {
        app.state.config.rules.len()
    }

    fn clamp_rules_select(&mut self, app: &App) {
        let len = Self::rules_count(app);
        if len == 0 {
            self.rules_select = 0;
            self.rules_scroll = 0;
        } else {
            self.rules_select = self.rules_select.min(len - 1);
        }
    }

    fn navigate_rules(&mut self, app: &App, step: i32) {
        let len = Self::rules_count(app);
        if len == 0 {
            return;
        }
        self.rules_select = (self.rules_select as i32 + step).rem_euclid(len as i32) as usize;
    }

    fn delete_rule(&mut self, app: &mut App) {
        let rules = &mut app.state.config.rules;
        if rules.is_empty() {
            return;
        }
        rules.remove(self.rules_select.min(rules.len() - 1));
        self.changed = true;
        self.clamp_rules_select(app);
    }

    fn start_edit_rule(&mut self, app: &App) {
        let Some(rule) = app.state.config.rules.get(self.rules_select) else {
            return;
        };
        self.editing = Some(EditState::rule(Some(self.rules_select), rule.clone()));
    }

    fn start_add_rule(&mut self, app: &App) {
        self.rules_select = Self::rules_count(app);
        self.editing = Some(EditState::rule(None, String::new()));
    }

    pub fn handle_key(&mut self, app: &mut App, key: KeyEvent) -> Option<Page> {
        match key.code {
            KeyCode::Esc => {
                if self.editing.is_some() {
                    self.editing = None;
                } else if self.view == View::Rules {
                    self.view = View::Fields;
                } else {
                    self.close(app);
                    return Some(Page::Main);
                }
            }
            KeyCode::Enter => {
                if self.editing.is_some() {
                    self.apply_edit(app);
                } else {
                    match self.view {
                        View::Fields => self.toggle_field(app),
                        View::Rules => self.start_edit_rule(app),
                    }
                }
            }
            KeyCode::Up => {
                if self.editing.is_some() {
                    return None;
                }
                match self.view {
                    View::Fields => {
                        self.fields_select = (self.fields_select + FIELD_COUNT - 1) % FIELD_COUNT;
                    }
                    View::Rules => self.navigate_rules(app, -1),
                }
            }
            KeyCode::Down => {
                if self.editing.is_some() {
                    return None;
                }
                match self.view {
                    View::Fields => self.fields_select = (self.fields_select + 1) % FIELD_COUNT,
                    View::Rules => self.navigate_rules(app, 1),
                }
            }
            KeyCode::Char('r') => {
                if self.editing.is_none() && self.view == View::Fields {
                    self.view = View::Rules;
                }
            }
            KeyCode::Char('a') => {
                if self.editing.is_none() && self.view == View::Rules {
                    self.start_add_rule(app);
                }
            }
            KeyCode::Char('d') => {
                if self.editing.is_none() && self.view == View::Rules {
                    self.delete_rule(app);
                }
            }
            KeyCode::Char(c) => {
                if let Some(edit) = self.editing.as_mut() {
                    if edit.rule.is_some() {
                        if edit.buffer.len() < 200 {
                            edit.buffer.push(c);
                        }
                    } else if c.is_ascii_digit() && edit.buffer.len() < 10 {
                        edit.buffer.push(c);
                    }
                }
            }
            KeyCode::Backspace => {
                if let Some(edit) = self.editing.as_mut() {
                    edit.buffer.pop();
                }
            }
            _ => {}
        }
        None
    }

    pub fn draw(&mut self, app: &mut App, f: &mut Frame) {
        let area = popup_rect(f.area());
        f.render_widget(Clear, area);

        let (title, hint) = match (self.view, self.editing.is_some()) {
            (View::Rules, false) => (
                "设置 · 规则",
                "↑↓ 导航  Enter 编辑  a 添加  d 删除  Esc 返回",
            ),
            (_, true) => ("设置", "Enter 确认  Esc 取消"),
            (View::Fields, false) => ("设置", "↑↓ 导航  Enter 编辑/切换  r 规则  Esc 保存并关闭"),
        };

        let block = Block::default()
            .title(title)
            .title_bottom(hint)
            .borders(Borders::ALL)
            .style(Style::default().fg(Color::White));
        let inner = block.inner(area);
        f.render_widget(block, area);

        let lines: Vec<Line> = match self.view {
            View::Fields => self.field_lines(app),
            View::Rules => self.rule_lines(app, inner.height as usize),
        };

        let paragraph = Paragraph::new(lines).style(Style::default().fg(Color::White));
        f.render_widget(paragraph, inner);
    }
}

impl SettingsWindow {
    fn field_lines(&self, app: &App) -> Vec<Line<'_>> {
        let mut lines = Vec::new();
        for (i, def) in FIELDS.iter().enumerate() {
            let selected = i == self.fields_select;
            let marker = if selected { ">> " } else { "   " };
            let mut value = self.field_value(app, def.kind);
            if let Some(edit) = &self.editing
                && edit.rule.is_none()
                && i == self.fields_select
            {
                value = format!("{}▌", edit.buffer);
            }
            let style = if selected {
                Style::default().fg(Color::Cyan)
            } else {
                Style::default().fg(Color::White)
            };
            lines.push(Line::from(vec![
                Span::styled(marker, style),
                Span::styled(pad_to(def.label, 12), style),
                Span::styled(format!("[{value}]"), style),
            ]));
        }
        lines
    }

    fn rule_lines(&mut self, app: &App, height: usize) -> Vec<Line<'_>> {
        let rules = &app.state.config.rules;
        let visible = height.saturating_sub(1).max(1);
        if rules.is_empty() {
            self.rules_scroll = 0;
            return vec![Line::from(Span::styled(
                "（暂无规则，按 a 添加）",
                Style::default().fg(Color::DarkGray),
            ))];
        }
        self.clamp_rules_select(app);
        if self.rules_select < self.rules_scroll {
            self.rules_scroll = self.rules_select;
        }
        if self.rules_select >= self.rules_scroll + visible {
            self.rules_scroll = self.rules_select + 1 - visible;
        }
        let mut lines = Vec::new();
        for (i, rule) in rules
            .iter()
            .enumerate()
            .skip(self.rules_scroll)
            .take(visible)
        {
            let selected = i == self.rules_select;
            let marker = if selected { ">> " } else { "   " };
            let mut text = rule.clone();
            if let Some(edit) = &self.editing
                && edit.rule.is_some()
                && edit.rule == Some(i)
            {
                text = format!("{}▌", edit.buffer);
            }
            let style = if selected {
                Style::default().fg(Color::Cyan)
            } else {
                Style::default().fg(Color::White)
            };
            lines.push(Line::from(vec![
                Span::styled(marker, style),
                Span::styled(text, style),
            ]));
        }
        lines
    }
}

fn cycle(values: &'static [&'static str], cur: &str) -> &'static str {
    let pos = values.iter().position(|v| *v == cur);
    match pos {
        Some(i) => values[(i + 1) % values.len()],
        None => values[0],
    }
}

const MODES: [&str; 3] = ["Rule", "Global", "Direct"];
const LOG_LEVELS: [&str; 5] = ["info", "debug", "warn", "error", "silent"];

#[derive(Clone, Copy, PartialEq)]
enum FieldKind {
    Mode,
    Port,
    SocksPort,
    AllowLan,
    LogLevel,
    UnifiedDelay,
    KeepAlive,
    Tun,
    Dns,
    Rules,
}

struct FieldDef {
    label: &'static str,
    kind: FieldKind,
}

const FIELDS: [FieldDef; 10] = [
    FieldDef {
        label: "模式",
        kind: FieldKind::Mode,
    },
    FieldDef {
        label: "混合端口",
        kind: FieldKind::Port,
    },
    FieldDef {
        label: "SOCKS 端口",
        kind: FieldKind::SocksPort,
    },
    FieldDef {
        label: "允许局域网",
        kind: FieldKind::AllowLan,
    },
    FieldDef {
        label: "日志级别",
        kind: FieldKind::LogLevel,
    },
    FieldDef {
        label: "统一延迟",
        kind: FieldKind::UnifiedDelay,
    },
    FieldDef {
        label: "保活间隔",
        kind: FieldKind::KeepAlive,
    },
    FieldDef {
        label: "TUN 模式",
        kind: FieldKind::Tun,
    },
    FieldDef {
        label: "DNS 模式",
        kind: FieldKind::Dns,
    },
    FieldDef {
        label: "规则编辑",
        kind: FieldKind::Rules,
    },
];

const FIELD_COUNT: usize = FIELDS.len();

fn on_off(v: bool) -> String {
    if v {
        "是".to_string()
    } else {
        "否".to_string()
    }
}

/// 按显示宽度补齐到指定宽度
fn pad_to(s: &str, width: usize) -> String {
    let pad = width.saturating_sub(display_width(s));
    format!("{s}{}", " ".repeat(pad))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cycle_wraps() {
        assert_eq!(cycle(&MODES, "Rule"), "Global");
        assert_eq!(cycle(&MODES, "Global"), "Direct");
        assert_eq!(cycle(&MODES, "Direct"), "Rule");
    }

    #[test]
    fn test_cycle_unknown_falls_back_to_first() {
        assert_eq!(cycle(&MODES, "unknown"), "Rule");
    }

    #[test]
    fn test_on_off() {
        assert_eq!(on_off(true), "是");
        assert_eq!(on_off(false), "否");
    }

    #[test]
    fn test_fields_all_unique_kinds() {
        let mut kinds: Vec<FieldKind> = FIELDS.iter().map(|f| f.kind).collect();
        kinds.dedup();
        assert_eq!(kinds.len(), FIELDS.len());
    }
}
