//! Unix 占位实现：暂不支持系统代理（Linux 一般走环境变量/发行版网络设置）。
use std::io::{Error, ErrorKind};

fn unsupported() -> Error {
    Error::new(
        ErrorKind::Unsupported,
        "暂不支持linux的系统代理喵，自己去source喵",
    )
}

/// 开启系统代理（不支持）
pub fn enable_proxy(_proxy_addr: &str) -> std::io::Result<()> {
    Err(unsupported())
}

/// 关闭系统代理（不支持）
pub fn disable_proxy() -> std::io::Result<()> {
    Err(unsupported())
}

/// 查询系统代理状态（不支持）
pub fn get_proxy_status() -> std::io::Result<(u32, String)> {
    Err(unsupported())
}
