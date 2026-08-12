//! 设置窗口：字段视图 + 规则子视图。
use crate::manager::Manager;
use crate::tui::Page;
use crate::tui::layout::{display_width, popup_rect, wrap_index};
use crate::window;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    Frame,
    style::{Color, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, Paragraph},
};

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

#[window(popup over Main)]
impl SettingsWindow {
    pub fn new(_manager: &Manager) -> Self {
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
    fn close(&mut self, manager: &Manager) {
        if self.changed {
            match manager.save_config() {
                Ok(()) => manager.reload_config(),
                Err(e) => manager.log_err(e),
            }
        }
    }

    fn toggle_field(&mut self, manager: &Manager) {
        let i = self.fields_select;
        if i >= FIELD_COUNT {
            return;
        }
        let kind = FIELDS[i].kind;
        match kind {
            FieldKind::Mode => {
                manager.edit_config(|c| c.mode = cycle(&MODES, &c.mode).to_string());
                self.changed = true;
            }
            FieldKind::LogLevel => {
                manager.edit_config(|c| c.log_level = cycle(&LOG_LEVELS, &c.log_level).to_string());
                self.changed = true;
            }
            FieldKind::AllowLan => {
                manager.edit_config(|c| c.allow_lan = !c.allow_lan);
                self.changed = true;
            }
            FieldKind::UnifiedDelay => {
                manager.edit_config(|c| c.unified_delay = !c.unified_delay);
                self.changed = true;
            }
            FieldKind::Tun => {
                let on = !manager.state_lock().tun_enabled();
                // 与 `T` 键（Manager::toggle_tun）共用同一份配置逻辑
                manager.edit_config(|c| c.set_tun_enabled(on));
                #[cfg(unix)]
                if on
                    && let Some(warn) = crate::core::mihomo::tun_capability_warning()
                {
                    manager.log_warn(warn);
                }
                self.changed = true;
            }
            FieldKind::Dns => {
                let on = !manager.state_lock().dns_enabled();
                manager.edit_config(|c| c.set_dns_enabled(on));
                self.changed = true;
            }
            FieldKind::Port | FieldKind::SocksPort | FieldKind::KeepAlive => {
                let buffer = self.field_value(manager, kind);
                self.editing = Some(EditState::field(buffer));
            }
            FieldKind::Rules => self.view = View::Rules,
        }
    }

    fn field_value(&self, manager: &Manager, kind: FieldKind) -> String {
        let config = &manager.state_lock().config;
        match kind {
            FieldKind::Mode => config.mode.clone(),
            FieldKind::Port => config.port.to_string(),
            FieldKind::SocksPort => config.socks_port.to_string(),
            FieldKind::AllowLan => on_off(config.allow_lan),
            FieldKind::LogLevel => config.log_level.clone(),
            FieldKind::UnifiedDelay => on_off(config.unified_delay),
            FieldKind::KeepAlive => config.keep_alive_interval.to_string(),
            FieldKind::Tun => on_off(config.tun.as_ref().is_some_and(|t| t.enable)),
            FieldKind::Dns => on_off(config.dns.as_ref().is_some_and(|d| d.enable)),
            FieldKind::Rules => format!("{} 条", config.rules.len()),
        }
    }

    /// 确认编辑：解析成功才写回配置
    fn apply_edit(&mut self, manager: &Manager) {
        let Some(edit) = self.editing.take() else {
            return;
        };
        if let Some(rule_idx) = edit.rule {
            if !edit.buffer.is_empty() {
                let applied = manager.edit_config(|c| {
                    if rule_idx < c.rules.len() {
                        c.rules[rule_idx] = edit.buffer.clone();
                        true
                    } else {
                        false
                    }
                });
                if applied {
                    self.changed = true;
                }
            }
            return;
        }
        match self.view {
            View::Rules => {
                // 新增规则
                if !edit.buffer.is_empty() {
                    manager.edit_config(|c| c.rules.push(edit.buffer.clone()));
                    self.changed = true;
                }
            }
            View::Fields => match FIELDS[self.fields_select].kind {
                FieldKind::Port => {
                    if let Ok(v) = edit.buffer.parse::<u16>() {
                        manager.edit_config(|c| c.port = v);
                        self.changed = true;
                    }
                }
                FieldKind::SocksPort => {
                    if let Ok(v) = edit.buffer.parse::<u16>() {
                        manager.edit_config(|c| c.socks_port = v);
                        self.changed = true;
                    }
                }
                FieldKind::KeepAlive => {
                    if let Ok(v) = edit.buffer.parse::<u32>() {
                        manager.edit_config(|c| c.keep_alive_interval = v);
                        self.changed = true;
                    }
                }
                _ => {}
            },
        }
    }

    fn rules_count(manager: &Manager) -> usize {
        manager.state_lock().config.rules.len()
    }

    fn clamp_rules_select(&mut self, manager: &Manager) {
        let len = Self::rules_count(manager);
        if len == 0 {
            self.rules_select = 0;
            self.rules_scroll = 0;
        } else {
            self.rules_select = self.rules_select.min(len - 1);
        }
    }

    fn navigate_rules(&mut self, manager: &Manager, step: i32) {
        let len = Self::rules_count(manager);
        if len == 0 {
            return;
        }
        self.rules_select = wrap_index(self.rules_select, len, step);
    }

    fn delete_rule(&mut self, manager: &Manager) {
        let len = Self::rules_count(manager);
        if len == 0 {
            return;
        }
        let idx = self.rules_select.min(len - 1);
        manager.edit_config(|c| c.rules.remove(idx));
        self.changed = true;
        self.clamp_rules_select(manager);
    }

    fn start_edit_rule(&mut self, manager: &Manager) {
        let rule = manager
            .state_lock()
            .config
            .rules
            .get(self.rules_select)
            .cloned();
        let Some(rule) = rule else {
            return;
        };
        self.editing = Some(EditState::rule(Some(self.rules_select), rule));
    }

    fn start_add_rule(&mut self, manager: &Manager) {
        self.rules_select = Self::rules_count(manager);
        self.editing = Some(EditState::rule(None, String::new()));
    }

    #[key(KeyCode::Esc, "保存并关闭", footer = false)]
    fn esc(&mut self, manager: &Manager) -> Option<Page> {
        if self.editing.is_some() {
            self.editing = None;
            None
        } else if self.view == View::Rules {
            self.view = View::Fields;
            None
        } else {
            self.close(manager);
            Some(Page::Main)
        }
    }

    #[key(KeyCode::Enter, "编辑/切换", footer = false)]
    fn enter(&mut self, manager: &Manager) -> Option<Page> {
        if self.editing.is_some() {
            self.apply_edit(manager);
        } else {
            match self.view {
                View::Fields => self.toggle_field(manager),
                View::Rules => self.start_edit_rule(manager),
            }
        }
        None
    }

    #[key(KeyCode::Up, "导航", footer = false)]
    fn up(&mut self, manager: &Manager) -> Option<Page> {
        if self.editing.is_none() {
            match self.view {
                View::Fields => {
                    self.fields_select = wrap_index(self.fields_select, FIELD_COUNT, -1);
                }
                View::Rules => self.navigate_rules(manager, -1),
            }
        }
        None
    }

    #[key(KeyCode::Down, "导航", footer = false)]
    fn down(&mut self, manager: &Manager) -> Option<Page> {
        if self.editing.is_none() {
            match self.view {
                View::Fields => self.fields_select = wrap_index(self.fields_select, FIELD_COUNT, 1),
                View::Rules => self.navigate_rules(manager, 1),
            }
        }
        None
    }

    #[key(KeyCode::Char('r'), "规则", footer = false)]
    fn show_rules(&mut self, _manager: &Manager) -> Option<Page> {
        if self.editing.is_none() && self.view == View::Fields {
            self.view = View::Rules;
        }
        None
    }

    #[key(KeyCode::Char('a'), "添加规则", footer = false)]
    fn add_rule(&mut self, manager: &Manager) -> Option<Page> {
        if self.editing.is_none() && self.view == View::Rules {
            self.start_add_rule(manager);
        }
        None
    }

    #[key(KeyCode::Char('d'), "删除规则", footer = false)]
    fn remove_rule(&mut self, manager: &Manager) -> Option<Page> {
        if self.editing.is_none() && self.view == View::Rules {
            self.delete_rule(manager);
        }
        None
    }

    #[key(KeyCode::Char(_))]
    fn input_char(&mut self, _manager: &Manager, key: KeyEvent) -> Option<Page> {
        if let Some(edit) = self.editing.as_mut()
            && let KeyCode::Char(c) = key.code
        {
            if edit.rule.is_some() {
                if edit.buffer.len() < 200 {
                    edit.buffer.push(c);
                }
            } else if c.is_ascii_digit() && edit.buffer.len() < 10 {
                edit.buffer.push(c);
            }
        }
        None
    }

    #[key(KeyCode::Backspace)]
    fn backspace(&mut self, _manager: &Manager) -> Option<Page> {
        if let Some(edit) = self.editing.as_mut() {
            edit.buffer.pop();
        }
        None
    }

    pub fn draw(&mut self, manager: &Manager, f: &mut Frame) {
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
            View::Fields => self.field_lines(manager),
            View::Rules => self.rule_lines(manager, inner.height as usize),
        };

        let paragraph = Paragraph::new(lines).style(Style::default().fg(Color::White));
        f.render_widget(paragraph, inner);
    }
}

impl SettingsWindow {
    fn field_lines(&self, manager: &Manager) -> Vec<Line<'_>> {
        let mut lines = Vec::new();
        for (i, def) in FIELDS.iter().enumerate() {
            let selected = i == self.fields_select;
            let marker = if selected { ">> " } else { "   " };
            let mut value = self.field_value(manager, def.kind);
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

    fn rule_lines(&mut self, manager: &Manager, height: usize) -> Vec<Line<'_>> {
        let rules: Vec<String> = manager.state_lock().config.rules.clone();
        let visible = height.saturating_sub(1).max(1);
        if rules.is_empty() {
            self.rules_scroll = 0;
            return vec![Line::from(Span::styled(
                "（暂无规则，按 a 添加）",
                Style::default().fg(Color::DarkGray),
            ))];
        }
        self.clamp_rules_select(manager);
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
