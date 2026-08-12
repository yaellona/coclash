//! Windows 实现：HKCU Internet Settings 注册表（WinINET 系统代理）。
use winreg::RegKey;
use winreg::enums::*;

const INTERNET_SETTINGS: &str = r"Software\Microsoft\Windows\CurrentVersion\Internet Settings";

/// 开启系统代理
pub fn enable_proxy(proxy_addr: &str) -> std::io::Result<()> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let internet_settings = hkcu.open_subkey_with_flags(INTERNET_SETTINGS, KEY_WRITE)?;
    internet_settings.set_value("ProxyEnable", &1u32)?;
    internet_settings.set_value("ProxyServer", &proxy_addr)?;

    Ok(())
}

/// 关闭系统代理
pub fn disable_proxy() -> std::io::Result<()> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let internet_settings = hkcu.open_subkey_with_flags(INTERNET_SETTINGS, KEY_WRITE)?;
    internet_settings.set_value("ProxyEnable", &0u32)?;
    Ok(())
}

/// 查询系统代理状态：返回 (ProxyEnable, ProxyServer)
pub fn get_proxy_status() -> std::io::Result<(u32, String)> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let internet_settings = hkcu.open_subkey_with_flags(INTERNET_SETTINGS, KEY_READ)?;

    let enable: u32 = internet_settings.get_value("ProxyEnable")?;
    let server: String = internet_settings
        .get_value("ProxyServer")
        .unwrap_or_default();
    Ok((enable, server))
}
