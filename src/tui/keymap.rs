//! 按键注册表与帮助/底部栏生成（纯元数据；分发由 `#[window]` 生成的 `handle_key` 完成）。
use crate::tui::Page;
use crossterm::event::KeyCode;

/// 聚合后的全局按键表，由 `crate::windows!` 注册表宏在 `ui/windows/mod.rs` 生成。
pub use super::windows::BINDINGS;

/// 窗口内按键定义（由 `#[key]` 收集）：不携带所属页，`mode` 由注册表宏填充。
#[derive(Clone, Copy)]
pub struct KeyDef {
    pub key: KeyCode,
    pub desc: Option<&'static str>,
    pub in_footer: bool,
}

/// 一条按键绑定：所属窗口 + 按键 + 帮助文案。
#[derive(Clone, Copy)]
pub struct Binding {
    pub mode: Page,
    pub key: KeyCode,
    pub desc: &'static str,
    pub in_footer: bool,
}

/// 按键对应的可读标签
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

/// 查表：命中返回绑定（仅测试使用）
#[cfg(test)]
pub fn lookup(mode: Page, key: KeyCode) -> Option<&'static Binding> {
    BINDINGS.iter().find(|b| b.mode == mode && b.key == key)
}

/// 收集指定筛选条件下的 (按键标签, 描述) 列表，相邻同描述项合并为 "↑/↓" 形式
fn collect_entries(filter: impl Fn(&Binding) -> bool) -> Vec<(String, &'static str)> {
    let mut entries: Vec<(String, &'static str)> = Vec::new();
    for b in BINDINGS.iter().filter(|b| filter(b)) {
        if let Some(last) = entries.last_mut()
            && last.1 == b.desc
        {
            last.0 = format!("{}/{}", last.0, key_label(b.key));
            continue;
        }
        entries.push((key_label(b.key), b.desc));
    }
    entries
}

/// 帮助弹窗行：某窗口下的全部按键（有描述才展示）
pub fn help_rows(mode: Page) -> Vec<(String, &'static str)> {
    collect_entries(|b| b.mode == mode && !b.desc.is_empty())
}

/// 底部栏快捷键文案：主窗口中标记 `footer` 的按键
pub fn footer_text() -> String {
    collect_entries(|b| b.mode == Page::Main && b.in_footer)
        .into_iter()
        .map(|(k, d)| format!("{k}: {d}"))
        .collect::<Vec<_>>()
        .join(" | ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_has_main_bindings() {
        assert!(!BINDINGS.is_empty());
        let main = Page::Main;
        assert!(lookup(main, KeyCode::Char('q')).is_some());
        assert!(lookup(main, KeyCode::Up).is_some());
        assert!(lookup(main, KeyCode::Tab).is_some());
        assert!(lookup(main, KeyCode::Esc).is_some());
        assert!(lookup(main, KeyCode::PageDown).is_some());
        assert!(lookup(Page::UrlInput, KeyCode::Enter).is_some());
        assert!(lookup(Page::MihomoLog, KeyCode::PageDown).is_some());
        assert!(lookup(Page::Help, KeyCode::Up).is_some());
        assert!(lookup(Page::Help, KeyCode::Down).is_some());
        assert!(lookup(Page::Help, KeyCode::PageDown).is_some());
        assert!(lookup(main, KeyCode::Char('x')).is_none());
        assert!(lookup(Page::UrlInput, KeyCode::Char('q')).is_none());
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
        let rows = help_rows(Page::Main);
        assert!(rows.iter().any(|(_, d)| *d == "退出"));
        assert!(rows.iter().any(|(k, _)| k == "Enter"));
        assert!(rows.iter().any(|(_, d)| *d == "测速"));
        assert!(rows.iter().any(|(_, d)| *d == "开关mihomo"));
        assert!(rows.iter().any(|(_, d)| *d == "切换面板"));
    }

    #[test]
    fn test_run_handler() {
        let binding = lookup(Page::Main, KeyCode::Char('q')).unwrap();
        assert_eq!(binding.desc, "退出");
        assert!(binding.in_footer);
    }
}
