//! 设置窗口：字段视图 + 规则子视图。
//!
//! 数据源约定：本窗口编辑/落盘的是本地 `config.yaml`（唯一可写源）。
//! 编辑动作数据化为 `ConfigChange` 返回给命令层；落盘 + reload 在关闭时
//! 通过 `Effect::SaveAndReload` 一次性完成。
//! 运行中若与 mihomo 运行时状态（心跳从 `/configs` 同步）不一致，
//! 在字段行尾以灰色 `（运行时: X）` 提示。
use crate::manager::commands::{ConfigChange, Effect};
use crate::manager::state::AppState;
use crate::tui::action::{Binding, KeyPattern};
use crate::tui::cmd::{Cmd, Nav};
use crate::tui::layout::{display_width, popup_rect, wrap_index};
use crate::tui::page::Page;
use crate::tui::pages::PageId;
use crossterm::event::{KeyCode, KeyEvent};
use ratatui::{
    Frame,
    style::{Color, Modifier, Style},
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
pub struct SettingsPage {
    view: View,
    fields_select: usize,
    rules_select: usize,
    rules_scroll: usize,
    editing: Option<EditState>,
    /// 是否有未落盘的改动（关闭时仅在有改动时写盘 + 重载）
    changed: bool,
}

impl SettingsPage {
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

    /// 关闭：有改动则落盘 + 重载，否则直接返回父页
    fn close(&mut self) -> Cmd {
        let effect = if self.changed {
            Effect::SaveAndReload
        } else {
            Effect::None
        };
        Cmd::new(Nav::Back, effect)
    }

    fn toggle_field(&mut self, state: &AppState) -> Cmd {
        let i = self.fields_select;
        if i >= FIELD_COUNT {
            return Cmd::none();
        }
        let kind = FIELDS[i].kind;
        let change = match kind {
            FieldKind::Mode => Some(ConfigChange::Mode(
                cycle(&MODES, &state.config.mode).to_string(),
            )),
            FieldKind::LogLevel => Some(ConfigChange::LogLevel(
                cycle(&LOG_LEVELS, &state.config.log_level).to_string(),
            )),
            FieldKind::AllowLan => Some(ConfigChange::AllowLan(!state.config.allow_lan)),
            FieldKind::UnifiedDelay => {
                Some(ConfigChange::UnifiedDelay(!state.config.unified_delay))
            }
            // 与 `T` 键（Manager::toggle_tun）共用同一份配置逻辑
            FieldKind::Tun => Some(ConfigChange::Tun(!state.tun_enabled())),
            FieldKind::Dns => Some(ConfigChange::Dns(!state.dns_enabled())),
            FieldKind::Port | FieldKind::SocksPort | FieldKind::KeepAlive => {
                let buffer = self.field_value(state, kind);
                self.editing = Some(EditState::field(buffer));
                None
            }
            FieldKind::Rules => {
                self.view = View::Rules;
                None
            }
        };
        match change {
            Some(change) => {
                self.changed = true;
                Cmd::effect(Effect::Config(change))
            }
            None => Cmd::none(),
        }
    }

    fn field_value(&self, state: &AppState, kind: FieldKind) -> String {
        let config = &state.config;
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

    /// 运行时一致性提示：运行中且 runtime 与文件值不一致时返回 `Some(运行时值)`
    fn runtime_value(&self, state: &AppState, kind: FieldKind) -> Option<String> {
        if !state.mihomo.runtime.api_ready {
            return None;
        }
        let runtime = match kind {
            FieldKind::Mode => state.mihomo.runtime.mode.clone(),
            FieldKind::Port => state.mihomo.runtime.mixed_port.map(|p| p.to_string()),
            FieldKind::SocksPort => state.mihomo.runtime.socks_port.map(|p| p.to_string()),
            FieldKind::Tun => state.mihomo.runtime.tun_enabled.map(on_off),
            FieldKind::Dns => state.mihomo.runtime.dns_enabled.map(on_off),
            _ => None,
        };
        let runtime = runtime?;
        let file = self.field_value(state, kind);
        (!file.eq_ignore_ascii_case(&runtime)).then_some(runtime)
    }

    /// 确认编辑：解析成功才产生配置修改
    fn apply_edit(&mut self, state: &AppState) -> Cmd {
        let Some(edit) = self.editing.take() else {
            return Cmd::none();
        };
        if let Some(rule_idx) = edit.rule {
            if !edit.buffer.is_empty() && rule_idx < state.config.rules.len() {
                self.changed = true;
                return Cmd::effect(Effect::Config(ConfigChange::SetRule {
                    index: rule_idx,
                    text: edit.buffer,
                }));
            }
            return Cmd::none();
        }
        match self.view {
            View::Rules => {
                // 新增规则
                if !edit.buffer.is_empty() {
                    self.changed = true;
                    return Cmd::effect(Effect::Config(ConfigChange::AddRule(edit.buffer)));
                }
                Cmd::none()
            }
            View::Fields => {
                let change = match FIELDS[self.fields_select].kind {
                    FieldKind::Port => edit.buffer.parse::<u16>().ok().map(ConfigChange::Port),
                    FieldKind::SocksPort => {
                        edit.buffer.parse::<u16>().ok().map(ConfigChange::SocksPort)
                    }
                    FieldKind::KeepAlive => {
                        edit.buffer.parse::<u32>().ok().map(ConfigChange::KeepAlive)
                    }
                    _ => None,
                };
                match change {
                    Some(change) => {
                        self.changed = true;
                        Cmd::effect(Effect::Config(change))
                    }
                    None => Cmd::none(),
                }
            }
        }
    }

    fn clamp_rules_select(&mut self, state: &AppState) {
        let len = state.config.rules.len();
        if len == 0 {
            self.rules_select = 0;
            self.rules_scroll = 0;
        } else {
            self.rules_select = self.rules_select.min(len - 1);
        }
    }

    fn navigate_rules(&mut self, state: &AppState, step: i32) {
        let len = state.config.rules.len();
        if len == 0 {
            return;
        }
        self.rules_select = wrap_index(self.rules_select, len, step);
    }

    fn delete_rule(&mut self, state: &AppState) -> Cmd {
        let len = state.config.rules.len();
        if len == 0 {
            return Cmd::none();
        }
        let index = self.rules_select.min(len - 1);
        self.changed = true;
        Cmd::effect(Effect::Config(ConfigChange::RemoveRule(index)))
    }

    fn start_edit_rule(&mut self, state: &AppState) -> Cmd {
        let Some(rule) = state.config.rules.get(self.rules_select).cloned() else {
            return Cmd::none();
        };
        self.editing = Some(EditState::rule(Some(self.rules_select), rule));
        Cmd::none()
    }

    fn start_add_rule(&mut self, state: &AppState) -> Cmd {
        self.rules_select = state.config.rules.len();
        self.editing = Some(EditState::rule(None, String::new()));
        Cmd::none()
    }

    fn esc(&mut self, _state: &AppState) -> Cmd {
        if self.editing.is_some() {
            self.editing = None;
            Cmd::none()
        } else if self.view == View::Rules {
            self.view = View::Fields;
            Cmd::none()
        } else {
            self.close()
        }
    }

    fn enter(&mut self, state: &AppState) -> Cmd {
        if self.editing.is_some() {
            self.apply_edit(state)
        } else {
            match self.view {
                View::Fields => self.toggle_field(state),
                View::Rules => self.start_edit_rule(state),
            }
        }
    }

    fn up(&mut self, state: &AppState) -> Cmd {
        if self.editing.is_none() {
            match self.view {
                View::Fields => {
                    self.fields_select = wrap_index(self.fields_select, FIELD_COUNT, -1);
                }
                View::Rules => self.navigate_rules(state, -1),
            }
        }
        Cmd::none()
    }

    fn down(&mut self, state: &AppState) -> Cmd {
        if self.editing.is_none() {
            match self.view {
                View::Fields => self.fields_select = wrap_index(self.fields_select, FIELD_COUNT, 1),
                View::Rules => self.navigate_rules(state, 1),
            }
        }
        Cmd::none()
    }

    fn show_rules(&mut self, _state: &AppState) -> Cmd {
        if self.editing.is_none() && self.view == View::Fields {
            self.view = View::Rules;
        }
        Cmd::none()
    }

    fn add_rule(&mut self, state: &AppState) -> Cmd {
        if self.editing.is_none() && self.view == View::Rules {
            self.start_add_rule(state)
        } else {
            Cmd::none()
        }
    }

    fn remove_rule(&mut self, state: &AppState) -> Cmd {
        if self.editing.is_none() && self.view == View::Rules {
            self.delete_rule(state)
        } else {
            Cmd::none()
        }
    }

    fn input_char(&mut self, _state: &AppState, key: KeyEvent) -> Cmd {
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
        Cmd::none()
    }

    fn backspace(&mut self, _state: &AppState) -> Cmd {
        if let Some(edit) = self.editing.as_mut() {
            edit.buffer.pop();
        }
        Cmd::none()
    }
}

impl Default for SettingsPage {
    fn default() -> Self {
        Self::new()
    }
}

impl Page for SettingsPage {
    const BINDINGS: &'static [Binding<Self>] = &[
        Binding::on(
            KeyPattern::Code(KeyCode::Esc),
            "保存并关闭",
            false,
            Self::esc,
        ),
        Binding::on(
            KeyPattern::Code(KeyCode::Enter),
            "编辑/切换",
            false,
            Self::enter,
        ),
        Binding::on(KeyPattern::Code(KeyCode::Up), "导航", false, Self::up),
        Binding::on(KeyPattern::Code(KeyCode::Down), "导航", false, Self::down),
        Binding::on(
            KeyPattern::Code(KeyCode::Char('r')),
            "规则",
            false,
            Self::show_rules,
        ),
        Binding::on(
            KeyPattern::Code(KeyCode::Char('a')),
            "添加规则",
            false,
            Self::add_rule,
        ),
        Binding::on(
            KeyPattern::Code(KeyCode::Char('d')),
            "删除规则",
            false,
            Self::remove_rule,
        ),
        Binding::text(KeyPattern::AnyChar, "", false, Self::input_char),
        Binding::on(
            KeyPattern::Code(KeyCode::Backspace),
            "",
            false,
            Self::backspace,
        ),
    ];

    fn parent(&self) -> Option<PageId> {
        Some(PageId::Main)
    }

    fn on_open(&mut self) {
        self.view = View::Fields;
        self.fields_select = 0;
        self.rules_select = 0;
        self.rules_scroll = 0;
        self.editing = None;
        self.changed = false;
    }

    fn draw(&mut self, state: &AppState, f: &mut Frame) {
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
            View::Fields => self.field_lines(state),
            View::Rules => self.rule_lines(state, inner.height as usize),
        };

        let paragraph = Paragraph::new(lines).style(Style::default().fg(Color::White));
        f.render_widget(paragraph, inner);
    }
}

impl SettingsPage {
    fn field_lines(&self, state: &AppState) -> Vec<Line<'_>> {
        let mut lines = Vec::new();
        for (i, def) in FIELDS.iter().enumerate() {
            let selected = i == self.fields_select;
            let marker = if selected { ">> " } else { "   " };
            let mut value = self.field_value(state, def.kind);
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
            let mut spans = vec![
                Span::styled(marker, style),
                Span::styled(pad_to(def.label, 12), style),
                Span::styled(format!("[{value}]"), style),
            ];
            // 运行时一致性提示（只读展示，编辑仍走文件配置）
            if let Some(runtime) = self.runtime_value(state, def.kind) {
                spans.push(Span::styled(
                    format!(" （运行时: {runtime}）"),
                    Style::default()
                        .fg(Color::DarkGray)
                        .add_modifier(Modifier::DIM),
                ));
            }
            lines.push(Line::from(spans));
        }
        lines
    }

    fn rule_lines(&mut self, state: &AppState, height: usize) -> Vec<Line<'_>> {
        let rules: Vec<String> = state.config.rules.clone();
        let visible = height.saturating_sub(1).max(1);
        if rules.is_empty() {
            self.rules_scroll = 0;
            return vec![Line::from(Span::styled(
                "（暂无规则，按 a 添加）",
                Style::default().fg(Color::DarkGray),
            ))];
        }
        self.clamp_rules_select(state);
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
    use crossterm::event::KeyModifiers;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

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

    #[test]
    fn test_toggle_mode_produces_config_change() {
        let state = AppState::test_fixture();
        let mut page = SettingsPage::new();
        // 第一个字段是模式，默认 Rule → Global
        let cmd = page.handle_key(&state, key(KeyCode::Enter));
        assert_eq!(
            cmd,
            Cmd::effect(Effect::Config(ConfigChange::Mode("Global".into())))
        );
        // 无改动时关闭直接返回父页
        let mut page = SettingsPage::new();
        assert_eq!(page.handle_key(&state, key(KeyCode::Esc)), Cmd::back());
    }

    #[test]
    fn test_close_saves_after_change() {
        let state = AppState::test_fixture();
        let mut page = SettingsPage::new();
        page.handle_key(&state, key(KeyCode::Enter));
        assert_eq!(
            page.handle_key(&state, key(KeyCode::Esc)),
            Cmd::new(Nav::Back, Effect::SaveAndReload)
        );
    }

    #[test]
    fn test_edit_port_applies_change() {
        let state = AppState::test_fixture();
        let mut page = SettingsPage::new();
        page.fields_select = 1; // 混合端口
        assert_eq!(page.handle_key(&state, key(KeyCode::Enter)), Cmd::none());
        // 清空原端口（默认 4 位）后输入 9999
        for _ in 0..4 {
            page.handle_key(&state, key(KeyCode::Backspace));
        }
        for _ in 0..4 {
            page.handle_key(&state, key(KeyCode::Char('9')));
        }
        let cmd = page.handle_key(&state, key(KeyCode::Enter));
        assert_eq!(cmd, Cmd::effect(Effect::Config(ConfigChange::Port(9999))));
    }

    #[test]
    fn test_rule_remove_after_entering_rules_view() {
        let state = AppState::test_fixture();
        let mut page = SettingsPage::new();
        assert_eq!(
            page.handle_key(&state, key(KeyCode::Char('r'))),
            Cmd::none()
        );
        let cmd = page.handle_key(&state, key(KeyCode::Char('d')));
        assert_eq!(
            cmd,
            Cmd::effect(Effect::Config(ConfigChange::RemoveRule(0)))
        );
    }
}
