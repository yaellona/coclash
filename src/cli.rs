//! 命令行分派：默认进入 TUI，`core` 子命令等价原生 mihomo。
//!
//! 解析只做「首参数」判断，`core` 之后的参数原样透传给 mihomo（含 `-d`/`-f`/`-v` 等），
//! 不注入任何默认参数；因此 `coclash core` 与直接运行 mihomo 行为一致。
use std::ffi::{OsStr, OsString};

/// 命令行解析结果
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Cli {
    /// 默认：进入 TUI
    Tui,
    /// `core [参数...]`：将参数透传给内嵌 mihomo
    Core(Vec<OsString>),
    /// `help` / `-h` / `--help`
    Help,
}

impl Cli {
    /// 解析除 argv[0] 之外的参数；未知参数保持兼容（回落 TUI，不报错）
    pub fn parse(args: impl IntoIterator<Item = OsString>) -> Self {
        let mut args = args.into_iter();
        match args.next().as_deref() {
            None => Self::Tui,
            Some(first) if first == OsStr::new("core") || first == OsStr::new("--core") => {
                Self::Core(args.collect())
            }
            Some(first)
                if first == OsStr::new("help")
                    || first == OsStr::new("-h")
                    || first == OsStr::new("--help") =>
            {
                Self::Help
            }
            Some(_) => Self::Tui,
        }
    }
}

/// `help` 子命令的用法文本
pub fn help_text() -> String {
    format!(
        r#"coclash {} - 基于 mihomo 内核的 TUI

用法:
  coclash               启动 TUI（默认）
  coclash core [参数]   直接运行内嵌 mihomo，参数原样透传
  coclash help          显示本帮助

示例:
  coclash core -v                 查看内核版本
  coclash core -d ~/.config/coclash -f config.yaml
"#,
        env!("CARGO_PKG_VERSION")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<OsString> {
        list.iter().map(OsString::from).collect()
    }

    #[test]
    fn test_empty_args_is_tui() {
        assert_eq!(Cli::parse(args(&[])), Cli::Tui);
    }

    #[test]
    fn test_unknown_arg_falls_back_to_tui() {
        assert_eq!(Cli::parse(args(&["--wat"])), Cli::Tui);
    }

    #[test]
    fn test_core_collects_rest_verbatim() {
        assert_eq!(
            Cli::parse(args(&["core", "-d", "/tmp/x", "-f", "config.yaml"])),
            Cli::Core(args(&["-d", "/tmp/x", "-f", "config.yaml"]))
        );
        assert_eq!(Cli::parse(args(&["core"])), Cli::Core(vec![]));
        // 透传参数里即使出现 core/--help 也不再二次解析
        assert_eq!(
            Cli::parse(args(&["core", "--help"])),
            Cli::Core(args(&["--help"]))
        );
    }

    #[test]
    fn test_core_alias() {
        assert_eq!(
            Cli::parse(args(&["--core", "-v"])),
            Cli::Core(args(&["-v"]))
        );
    }

    #[test]
    fn test_help() {
        assert_eq!(Cli::parse(args(&["help"])), Cli::Help);
        assert_eq!(Cli::parse(args(&["-h"])), Cli::Help);
        assert_eq!(Cli::parse(args(&["--help"])), Cli::Help);
        assert!(help_text().contains("coclash core"));
    }
}
