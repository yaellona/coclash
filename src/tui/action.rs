//! 按键绑定接口：快捷键 + 执行函数 + 描述。
//!
//! 页面通过 `Page::BINDINGS` 声明一组 `Binding`：
//! - `Binding::on`：普通处理器，签名 `fn(&mut W, &AppState) -> Cmd`；
//! - `Binding::text`：需要原始按键的处理器（文本输入），签名 `fn(&mut W, &AppState, KeyEvent) -> Cmd`。
//!
//! 帮助弹窗/底部栏据此自动生成文案；按键分发由 `page::dispatch` 完成
//! （精确优先、通配兜底，声明顺序无关）。
use crate::manager::state::AppState;
use crate::tui::cmd::Cmd;
use crate::tui::pages::{BINDINGS, PageId};
use crossterm::event::{KeyCode, KeyEvent};

/// 按键模式：精确按键 或 任意字符（输入类页面用 `AnyChar` 捕获文本输入）。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum KeyPattern {
    Code(KeyCode),
    AnyChar,
}

impl KeyPattern {
    pub fn matches(self, code: KeyCode) -> bool {
        match self {
            KeyPattern::Code(c) => c == code,
            KeyPattern::AnyChar => matches!(code, KeyCode::Char(_)),
        }
    }

    /// 按键的可读标签（帮助/底栏展示）
    pub fn label(self) -> String {
        match self {
            KeyPattern::Code(key) => key_label(key),
            KeyPattern::AnyChar => "输入".to_string(),
        }
    }
}

/// 按键处理器：分型避免所有 handler 都携带 `KeyEvent`
pub enum Handler<W> {
    Plain(fn(&mut W, &AppState) -> Cmd),
    Keyed(fn(&mut W, &AppState, KeyEvent) -> Cmd),
}

/// 一条按键绑定：快捷键 + 执行函数 + 描述。
pub struct Binding<W> {
    pub key: KeyPattern,
    /// 帮助文案；空串 = 不进入帮助/底栏（如纯文本输入用的通配按键）
    pub desc: &'static str,
    /// 是否显示在底部栏
    pub footer: bool,
    pub(crate) handler: Handler<W>,
}

impl<W> Binding<W> {
    /// 普通按键：`fn(&mut W, &AppState) -> Cmd`
    pub const fn on(
        key: KeyPattern,
        desc: &'static str,
        footer: bool,
        run: fn(&mut W, &AppState) -> Cmd,
    ) -> Self {
        Self {
            key,
            desc,
            footer,
            handler: Handler::Plain(run),
        }
    }

    /// 需要原始按键的处理器（如文本输入）：`fn(&mut W, &AppState, KeyEvent) -> Cmd`
    pub const fn text(
        key: KeyPattern,
        desc: &'static str,
        footer: bool,
        run: fn(&mut W, &AppState, KeyEvent) -> Cmd,
    ) -> Self {
        Self {
            key,
            desc,
            footer,
            handler: Handler::Keyed(run),
        }
    }
}

/// 按键的纯元数据（去掉执行函数），供帮助/底栏聚合。
#[derive(Clone, Copy)]
pub struct BindingMeta {
    pub page: PageId,
    pub key: KeyPattern,
    pub desc: &'static str,
    pub footer: bool,
}

/// 把某页的绑定表转为元数据（desc 为空的不进入帮助/底栏）
pub fn binding_meta<W>(page: PageId, bindings: &'static [Binding<W>]) -> Vec<BindingMeta> {
    bindings
        .iter()
        .filter(|b| !b.desc.is_empty())
        .map(|b| BindingMeta {
            page,
            key: b.key,
            desc: b.desc,
            footer: b.footer,
        })
        .collect()
}

/// 按绑定表分发按键：精确匹配优先，其次 `AnyChar` 兜底
pub fn dispatch<W>(
    bindings: &'static [Binding<W>],
    page: &mut W,
    state: &AppState,
    key: KeyEvent,
) -> Cmd {
    let code = key.code;
    let hit = bindings
        .iter()
        .find(|b| matches!(b.key, KeyPattern::Code(c) if c == code))
        .or_else(|| bindings.iter().find(|b| b.key.matches(code)));
    match hit.map(|b| &b.handler) {
        Some(Handler::Plain(run)) => run(page, state),
        Some(Handler::Keyed(run)) => run(page, state, key),
        None => Cmd::none(),
    }
}

/// `KeyCode` 的可读标签
pub fn key_label(key: KeyCode) -> String {
    match key {
        KeyCode::Char(c) => c.to_string(),
        KeyCode::Enter => "Enter".to_string(),
        KeyCode::Esc => "Esc".to_string(),
        KeyCode::Backspace => "Backspace".to_string(),
        KeyCode::Up => "↑".to_string(),
        KeyCode::Down => "↓".to_string(),
        KeyCode::Left => "←".to_string(),
        KeyCode::Right => "→".to_string(),
        KeyCode::PageUp => "PageUp".to_string(),
        KeyCode::PageDown => "PageDown".to_string(),
        KeyCode::Home => "Home".to_string(),
        KeyCode::End => "End".to_string(),
        KeyCode::Delete => "Del".to_string(),
        KeyCode::Insert => "Ins".to_string(),
        KeyCode::Tab => "Tab".to_string(),
        KeyCode::BackTab => "BackTab".to_string(),
        KeyCode::F(n) => format!("F{n}"),
        _ => format!("{key:?}"),
    }
}

/// 收集某筛选条件下的 (按键标签, 描述)，相邻同描述项合并为 "↑/↓" 形式
fn collect_entries(
    meta: &[BindingMeta],
    filter: impl Fn(&BindingMeta) -> bool,
) -> Vec<(String, &'static str)> {
    let mut entries: Vec<(String, &'static str)> = Vec::new();
    for b in meta.iter().filter(|b| filter(b)) {
        if let Some(last) = entries.last_mut()
            && last.1 == b.desc
        {
            last.0 = format!("{}/{}", last.0, b.key.label());
            continue;
        }
        entries.push((b.key.label(), b.desc));
    }
    entries
}

/// 帮助弹窗行：指定页下全部按键（有描述才展示）
pub fn help_rows(page: PageId) -> Vec<(String, &'static str)> {
    collect_entries(&BINDINGS, |b| b.page == page)
}

/// 底部栏快捷键文案：主页面中标记 `footer` 的按键
pub fn footer_text() -> String {
    collect_entries(&BINDINGS, |b| b.page == PageId::Main && b.footer)
        .into_iter()
        .map(|(k, d)| format!("{k}: {d}"))
        .collect::<Vec<_>>()
        .join(" | ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_key_pattern_matches() {
        assert!(KeyPattern::Code(KeyCode::Char('q')).matches(KeyCode::Char('q')));
        assert!(!KeyPattern::Code(KeyCode::Char('q')).matches(KeyCode::Char('x')));
        assert!(KeyPattern::AnyChar.matches(KeyCode::Char('a')));
        assert!(!KeyPattern::AnyChar.matches(KeyCode::Enter));
    }

    #[test]
    fn test_footer_and_help_generated() {
        let footer = footer_text();
        assert!(footer.contains("q: 退出"));
        assert!(footer.contains("↑/↓: 导航"));
        assert!(footer.contains("?: 帮助"));
        assert!(!footer.contains("开关mihomo"));
        assert!(!footer.contains("测速"));
        assert!(!footer.contains("TUN"));
        assert!(!footer.contains("切换面板"));
        let rows = help_rows(PageId::Main);
        assert!(rows.iter().any(|(_, d)| *d == "退出"));
        assert!(rows.iter().any(|(k, _)| k == "Enter"));
        assert!(rows.iter().any(|(_, d)| *d == "测速"));
        assert!(rows.iter().any(|(_, d)| *d == "开关mihomo"));
        assert!(rows.iter().any(|(_, d)| *d == "切换面板"));
    }
}
