use crate::app::Manager;
use crate::app::WindowId;
use crate::app::keymap::{Binding, popup};
use crate::app::tasks;
use crate::app::ui::pages::main::MAIN;
use crate::app::ui::pages::{display_width, popup_rect};
use crate::app::ui::{Popup, Window, WindowCtx};
use crate::constants::SETTINGS_FILE;
use crate::operation_log::LogType;
use crossterm::event::KeyCode;
use ratatui::{
    Frame,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};
use std::sync::LazyLock;

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
}

impl SettingsWindow {
    pub(crate) fn new(_ctx: &WindowCtx) -> Self {
        Self {
            view: View::Fields,
            fields_select: 0,
            rules_select: 0,
            rules_scroll: 0,
            editing: None,
        }
    }

    fn reset(&mut self) {
        self.view = View::Fields;
        self.fields_select = 0;
        self.rules_select = 0;
        self.rules_scroll = 0;
        self.editing = None;
    }

    /// Esc 关闭：统一写盘 + 重载 mihomo；端口变更同步到 settings
    fn close(&mut self, m: &mut Manager) {
        m.config.settings.mixed_port = m.config.config.port;
        m.config.settings.socks_port = m.config.config.socks_port;
        if let Some(parent) = m.config.config_path.parent() {
            let settings_path = parent.join(SETTINGS_FILE);
            if let Ok(json) = serde_json::to_string_pretty(&m.config.settings) {
                let _ = std::fs::write(settings_path, json);
            }
        }
        match m.config.config.write_to_path(&m.config.config_path) {
            Ok(()) => {
                m.current_window = MAIN;
                tasks::reload(
                    m.tasks.tx.clone(),
                    m.config.settings.clone(),
                    m.config.config_path.clone(),
                );
            }
            Err(e) => m.logs.add(LogType::Error, e),
        }
    }

    fn toggle_field(&mut self, m: &mut Manager) {
        let i = self.fields_select;
        if i >= FIELD_COUNT {
            return;
        }
        let kind = FIELDS[i].kind;
        match kind {
            FieldKind::Mode => {
                m.config.config.mode = cycle(&MODES, &m.config.config.mode).to_string()
            }
            FieldKind::LogLevel => {
                m.config.config.log_level =
                    cycle(&LOG_LEVELS, &m.config.config.log_level).to_string()
            }
            FieldKind::AllowLan => m.config.config.allow_lan = !m.config.config.allow_lan,
            FieldKind::UnifiedDelay => {
                m.config.config.unified_delay = !m.config.config.unified_delay
            }
            FieldKind::Tun => {
                let on = !m.config.config.tun.as_ref().is_some_and(|t| t.enable);
                if on {
                    let tun = m
                        .config
                        .config
                        .tun
                        .get_or_insert_with(crate::config::mihomo_config::Tun::default_enabled);
                    tun.enable = true;
                    m.config
                        .config
                        .dns
                        .get_or_insert_with(crate::config::mihomo_config::Dns::default_enabled)
                        .enable = true;
                } else if let Some(t) = m.config.config.tun.as_mut() {
                    t.enable = false;
                }
            }
            FieldKind::Dns => {
                let on = !m.config.config.dns.as_ref().is_some_and(|d| d.enable);
                if on {
                    m.config
                        .config
                        .dns
                        .get_or_insert_with(crate::config::mihomo_config::Dns::default_enabled)
                        .enable = true;
                } else if let Some(d) = m.config.config.dns.as_mut() {
                    d.enable = false;
                }
            }
            FieldKind::Port | FieldKind::SocksPort | FieldKind::KeepAlive => {
                let buffer = self.field_value(m, kind);
                self.editing = Some(EditState::field(buffer));
            }
            FieldKind::Rules => self.view = View::Rules,
        }
    }

    fn field_value(&self, m: &Manager, kind: FieldKind) -> String {
        match kind {
            FieldKind::Mode => m.config.config.mode.clone(),
            FieldKind::Port => m.config.config.port.to_string(),
            FieldKind::SocksPort => m.config.config.socks_port.to_string(),
            FieldKind::AllowLan => on_off(m.config.config.allow_lan),
            FieldKind::LogLevel => m.config.config.log_level.clone(),
            FieldKind::UnifiedDelay => on_off(m.config.config.unified_delay),
            FieldKind::KeepAlive => m.config.config.keep_alive_interval.to_string(),
            FieldKind::Tun => on_off(m.config.config.tun.as_ref().is_some_and(|t| t.enable)),
            FieldKind::Dns => on_off(m.config.config.dns.as_ref().is_some_and(|d| d.enable)),
            FieldKind::Rules => format!("{} 条", m.config.config.rules.len()),
        }
    }

    /// 确认编辑：解析成功才写回配置
    fn apply_edit(&mut self, m: &mut Manager) {
        let Some(edit) = self.editing.take() else {
            return;
        };
        if let Some(rule_idx) = edit.rule {
            if !edit.buffer.is_empty() {
                let rules = &mut m.config.config.rules;
                if rule_idx < rules.len() {
                    rules[rule_idx] = edit.buffer.clone();
                }
            }
            return;
        }
        match self.view {
            View::Rules => {
                // 新增规则
                if !edit.buffer.is_empty() {
                    m.config.config.rules.push(edit.buffer.clone());
                }
            }
            View::Fields => match FIELDS[self.fields_select].kind {
                FieldKind::Port => {
                    if let Ok(v) = edit.buffer.parse::<u16>() {
                        m.config.config.port = v;
                    }
                }
                FieldKind::SocksPort => {
                    if let Ok(v) = edit.buffer.parse::<u16>() {
                        m.config.config.socks_port = v;
                    }
                }
                FieldKind::KeepAlive => {
                    if let Ok(v) = edit.buffer.parse::<u32>() {
                        m.config.config.keep_alive_interval = v;
                    }
                }
                _ => {}
            },
        }
    }

    fn rules_count(m: &Manager) -> usize {
        m.config.config.rules.len()
    }

    fn clamp_rules_select(&mut self, m: &Manager) {
        let len = Self::rules_count(m);
        if len == 0 {
            self.rules_select = 0;
            self.rules_scroll = 0;
        } else {
            self.rules_select = self.rules_select.min(len - 1);
        }
    }

    fn navigate_rules(&mut self, m: &Manager, step: i32) {
        let len = Self::rules_count(m);
        if len == 0 {
            return;
        }
        self.rules_select = (self.rules_select as i32 + step).rem_euclid(len as i32) as usize;
    }

    fn delete_rule(&mut self, m: &mut Manager) {
        let rules = &mut m.config.config.rules;
        if rules.is_empty() {
            return;
        }
        rules.remove(self.rules_select.min(rules.len() - 1));
        self.clamp_rules_select(m);
    }

    fn start_edit_rule(&mut self, m: &Manager) {
        let Some(rule) = m.config.config.rules.get(self.rules_select) else {
            return;
        };
        self.editing = Some(EditState::rule(Some(self.rules_select), rule.clone()));
    }

    fn start_add_rule(&mut self, m: &Manager) {
        self.rules_select = Self::rules_count(m);
        self.editing = Some(EditState::rule(None, String::new()));
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

/// 简单显示宽度：ASCII 1，其余按 2（本项目文案只有 ASCII + CJK）
fn pad_to(s: &str, width: usize) -> String {
    let pad = width.saturating_sub(display_width(s));
    format!("{s}{}", " ".repeat(pad))
}

#[popup(name = "settings")]
impl SettingsWindow {
    #[key(KeyCode::Esc, desc = "保存并关闭")]
    fn key_esc(&mut self, m: &mut Manager) {
        if self.editing.is_some() {
            self.editing = None;
        } else if self.view == View::Rules {
            self.view = View::Fields;
        } else {
            self.close(m);
        }
    }

    #[key(KeyCode::Enter, desc = "编辑/切换")]
    fn key_enter(&mut self, m: &mut Manager) {
        if self.editing.is_some() {
            self.apply_edit(m);
        } else {
            match self.view {
                View::Fields => self.toggle_field(m),
                View::Rules => self.start_edit_rule(m),
            }
        }
    }

    #[key(KeyCode::Up, desc = "导航")]
    fn key_up(&mut self, m: &mut Manager) {
        if self.editing.is_some() {
            return;
        }
        match self.view {
            View::Fields => {
                self.fields_select = (self.fields_select + FIELD_COUNT - 1) % FIELD_COUNT;
            }
            View::Rules => self.navigate_rules(m, -1),
        }
    }

    #[key(KeyCode::Down, desc = "导航")]
    fn key_down(&mut self, m: &mut Manager) {
        if self.editing.is_some() {
            return;
        }
        match self.view {
            View::Fields => self.fields_select = (self.fields_select + 1) % FIELD_COUNT,
            View::Rules => self.navigate_rules(m, 1),
        }
    }

    #[key('r', desc = "规则")]
    fn key_rules(&mut self, _m: &mut Manager) {
        if self.editing.is_none() && self.view == View::Fields {
            self.view = View::Rules;
        }
    }

    #[key('a', desc = "添加规则")]
    fn key_add_rule(&mut self, m: &mut Manager) {
        if self.editing.is_none() && self.view == View::Rules {
            self.start_add_rule(m);
        }
    }

    #[key('d', desc = "删除规则")]
    fn key_delete_rule(&mut self, m: &mut Manager) {
        if self.editing.is_none() && self.view == View::Rules {
            self.delete_rule(m);
        }
    }

    #[fallback]
    fn key_type(&mut self, _m: &mut Manager, key: KeyCode) {
        let Some(edit) = self.editing.as_mut() else {
            return;
        };
        match key {
            KeyCode::Char(c) => {
                if edit.rule.is_some() {
                    if edit.buffer.len() < 200 {
                        edit.buffer.push(c);
                    }
                } else if c.is_ascii_digit() && edit.buffer.len() < 10 {
                    edit.buffer.push(c);
                }
            }
            KeyCode::Backspace => {
                edit.buffer.pop();
            }
            _ => {}
        }
    }

    /// 打开时重置到字段视图
    #[on_open]
    fn on_open(&mut self, _m: &mut Manager) {
        self.reset();
    }

    #[render]
    fn draw(&mut self, m: &mut Manager, f: &mut Frame) {
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
            View::Fields => self.field_lines(m),
            View::Rules => self.rule_lines(m, inner.height as usize),
        };

        let paragraph = Paragraph::new(lines).style(Style::default().fg(Color::White));
        f.render_widget(paragraph, inner);
    }
}

impl SettingsWindow {
    fn field_lines(&self, m: &Manager) -> Vec<Line<'_>> {
        let mut lines = Vec::new();
        for (i, def) in FIELDS.iter().enumerate() {
            let selected = i == self.fields_select;
            let marker = if selected { ">> " } else { "   " };
            let mut value = self.field_value(m, def.kind);
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

    fn rule_lines(&mut self, m: &Manager, height: usize) -> Vec<Line<'_>> {
        let rules = &m.config.config.rules;
        let visible = height.saturating_sub(1).max(1);
        if rules.is_empty() {
            self.rules_scroll = 0;
            return vec![Line::from(Span::styled(
                "（暂无规则，按 a 添加）",
                Style::default().fg(Color::DarkGray),
            ))];
        }
        self.clamp_rules_select(m);
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
