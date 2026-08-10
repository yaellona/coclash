#[cfg(target_os = "windows")]
use winreg::RegKey;
#[cfg(target_os = "windows")]
use winreg::enums::*;

/// 开启系统代理（仅 windows 支持，linux 走环境变量自行 source）
#[cfg_attr(not(target_os = "windows"), allow(unused_variables))]
pub fn enable_proxy(proxy_addr: &str) -> std::io::Result<()> {
    #[cfg(target_os = "windows")]
    {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let internet_settings = hkcu.open_subkey_with_flags(
            r"Software\Microsoft\Windows\CurrentVersion\Internet Settings",
            KEY_WRITE,
        )?;
        internet_settings.set_value("ProxyEnable", &1u32)?;
        internet_settings.set_value("ProxyServer", &proxy_addr)?;

        Ok(())
    }

    #[cfg(not(target_os = "windows"))]
    {
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "暂不支持linux的系统代理喵，自己去source喵",
        ))
    }
}

/// 关闭系统代理
pub fn disable_proxy() -> std::io::Result<()> {
    #[cfg(target_os = "windows")]
    {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let internet_settings = hkcu.open_subkey_with_flags(
            r"Software\Microsoft\Windows\CurrentVersion\Internet Settings",
            KEY_WRITE,
        )?;
        internet_settings.set_value("ProxyEnable", &0u32)?;
        Ok(())
    }

    #[cfg(not(target_os = "windows"))]
    {
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "暂不支持linux的系统代理喵，自己去source喵",
        ))
    }
}

/// 查询系统代理状态：返回 (ProxyEnable, ProxyServer)
pub fn get_proxy_status() -> std::io::Result<(u32, String)> {
    #[cfg(target_os = "windows")]
    {
        let hkcu = RegKey::predef(HKEY_CURRENT_USER);
        let internet_settings = hkcu.open_subkey_with_flags(
            r"Software\Microsoft\Windows\CurrentVersion\Internet Settings",
            KEY_READ,
        )?;

        let enable: u32 = internet_settings.get_value("ProxyEnable")?;
        let server: String = internet_settings
            .get_value("ProxyServer")
            .unwrap_or_default();
        Ok((enable, server))
    }

    #[cfg(not(target_os = "windows"))]
    {
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "暂不支持linux的系统代理喵，自己去source喵",
        ))
    }
}
