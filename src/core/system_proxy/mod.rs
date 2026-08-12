//! 系统代理开关。
//!
//! Windows：写注册表（HKCU Internet Settings）；其他平台暂不支持
//! （Linux 一般走发行版网络设置或环境变量，见 unix 子模块）。
//! 三个函数的签名跨平台一致，调用方无需关心平台。

#[cfg(windows)]
mod windows;
#[cfg(unix)]
mod unix;

#[cfg(windows)]
pub use windows::{disable_proxy, enable_proxy, get_proxy_status};
#[cfg(unix)]
pub use unix::{disable_proxy, enable_proxy, get_proxy_status};
