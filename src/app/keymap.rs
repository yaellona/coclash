use crate::app::ui::pages;
use crate::app::WindowId;
use crossterm::event::KeyCode;
pub use ui_derive::{popup, window};
use std::sync::LazyLock;

/// 一条按键绑定：所属窗口 + 按键 + 帮助文案。
///
/// 纯元数据：按键分发由各窗口的 `handle_key` 完成，此处仅用于
/// 帮助弹窗和底部栏的自动生成。
#[derive(Clone, Copy)]
pub struct Binding {
    pub mode: WindowId,
    pub key: KeyCode,
    pub desc: &'static str,
    pub in_footer: bool,
}

/// 全局按键注册表：聚合各窗口的注册表。
/// 各窗口的按键定义在 `ui/pages/` 对应文件里（`#[window]` 声明），
/// 这里只负责合并，帮助弹窗和底部栏据此自动生成。
pub static BINDINGS: LazyLock<Vec<Binding>> = LazyLock::new(|| {
    let mut bindings = Vec::new();
    bindings.extend(pages::main::MAIN_BINDINGS.iter().copied());
    bindings.extend(pages::url_input::URL_INPUT_BINDINGS.iter().copied());
    bindings.extend(pages::provider_select::PROVIDER_SELECT_BINDINGS.iter().copied());
    bindings.extend(pages::help::HELP_BINDINGS.iter().copied());
    bindings.extend(pages::mihomo_log::MIHOMO_LOG_BINDINGS.iter().copied());
    bindings
});

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

/// 查表：命中返回绑定（仅测试使用；运行时分发走各窗口的 handle_key）
#[cfg(test)]
pub fn lookup(mode: WindowId, key: KeyCode) -> Option<&'static Binding> {
    BINDINGS
        .iter()
        .find(|b| b.mode == mode && b.key == key)
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
pub fn help_rows(mode: WindowId) -> Vec<(String, &'static str)> {
    collect_entries(|b| b.mode == mode && !b.desc.is_empty())
}

/// 底部栏快捷键文案：主窗口中标记 `footer` 的按键
pub fn footer_text() -> String {
    collect_entries(|b| b.mode == pages::main::MAIN && b.in_footer)
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
        let main = pages::main::MAIN;
        assert!(lookup(main, KeyCode::Char('q')).is_some());
        assert!(lookup(main, KeyCode::Up).is_some());
        assert!(lookup(main, KeyCode::Tab).is_some());
        assert!(lookup(main, KeyCode::Esc).is_some());
        assert!(lookup(main, KeyCode::PageDown).is_some());
        assert!(lookup(pages::url_input::URL_INPUT, KeyCode::Enter).is_some());
        assert!(lookup(pages::mihomo_log::MIHOMO_LOG, KeyCode::PageDown).is_some());
        assert!(lookup(pages::help::HELP, KeyCode::Up).is_some());
        assert!(lookup(pages::help::HELP, KeyCode::Down).is_some());
        assert!(lookup(pages::help::HELP, KeyCode::PageDown).is_some());
        assert!(lookup(main, KeyCode::Char('x')).is_none());
        assert!(lookup(pages::url_input::URL_INPUT, KeyCode::Char('q')).is_none());
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
        let rows = help_rows(pages::main::MAIN);
        assert!(rows.iter().any(|(_, d)| *d == "退出"));
        assert!(rows.iter().any(|(k, _)| k == "Enter"));
        assert!(rows.iter().any(|(_, d)| *d == "测速"));
        assert!(rows.iter().any(|(_, d)| *d == "开关mihomo"));
        assert!(rows.iter().any(|(_, d)| *d == "切换面板"));
    }

    #[test]
    fn test_run_handler() {
        let binding = lookup(pages::main::MAIN, KeyCode::Char('q')).unwrap();
        assert_eq!(binding.desc, "退出");
        assert!(binding.in_footer);
    }
}
