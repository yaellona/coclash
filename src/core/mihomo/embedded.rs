//! 内嵌 mihomo：编译期压缩嵌入（gz），运行期按需解压到内存或缓存目录。
//!
//! 这是 mihomo 的**唯一来源**：没有环境变量 / PATH / Nix wrapper 解析链。
//! 默认由 `process` 平台层解压到内存执行（Linux memfd / Windows 自删除临时文件）；
//! 仅在兜底或强制落盘时释放到 `{cache_dir}/coclash/bin/{sha8}/mihomo[.exe]`
//! （缓存根可用 `COCLASH_MIHOMO_DIR` 覆盖，同时视为强制落盘）。
//!
//! 未启用 `embed-mihomo` feature 时提供同签名 stub，保证编译并给出明确报错。
use crate::error::Error;
use std::path::PathBuf;

#[cfg(feature = "embed-mihomo")]
use std::fs;
#[cfg(feature = "embed-mihomo")]
use std::io::Read;
#[cfg(feature = "embed-mihomo")]
use std::path::Path;

#[cfg(feature = "embed-mihomo")]
const EMBEDDED_GZ: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/mihomo.gz"));

/// 解压内嵌 mihomo 到内存；未启用 `embed-mihomo` 时返回明确错误。
pub(crate) fn decompress_embedded() -> Result<Vec<u8>, Error> {
    #[cfg(feature = "embed-mihomo")]
    {
        decompress(EMBEDDED_GZ)
    }
    #[cfg(not(feature = "embed-mihomo"))]
    {
        Err(Error::Process(
            "本二进制未嵌入 mihomo（构建时请启用 embed-mihomo feature）".to_string(),
        ))
    }
}

/// 纯函数：gz → 原始字节（内存执行与落盘共用同一解压实现）
#[cfg(feature = "embed-mihomo")]
pub(crate) fn decompress(gz: &[u8]) -> Result<Vec<u8>, Error> {
    let mut raw = Vec::new();
    flate2::read::GzDecoder::new(gz).read_to_end(&mut raw)?;
    Ok(raw)
}

/// 内嵌 mihomo 的版本（编译期确定）
#[cfg(feature = "embed-mihomo")]
pub const VERSION: &str = env!("COCLASH_EMBEDDED_MIHOMO_VERSION");

#[cfg(not(feature = "embed-mihomo"))]
pub const VERSION: &str = "未嵌入";

#[cfg(feature = "embed-mihomo")]
const SHA256: &str = env!("COCLASH_EMBEDDED_MIHOMO_SHA256");
#[cfg(feature = "embed-mihomo")]
const SIZE: &str = env!("COCLASH_EMBEDDED_MIHOMO_SIZE");

/// 释放内嵌 mihomo 到缓存目录并返回可执行文件路径（已存在且大小匹配则复用）。
pub fn ensure_extracted() -> Result<PathBuf, Error> {
    #[cfg(feature = "embed-mihomo")]
    {
        let dir = cache_root()?.join("coclash").join("bin").join(short_sha());
        let size: u64 = SIZE
            .parse()
            .map_err(|_| Error::Process("内嵌 mihomo 大小信息损坏".to_string()))?;
        ensure_extracted_with(EMBEDDED_GZ, &dir, size)
    }
    #[cfg(not(feature = "embed-mihomo"))]
    {
        Err(Error::Process(
            "本二进制未嵌入 mihomo（构建时请启用 embed-mihomo feature）".to_string(),
        ))
    }
}

#[cfg(feature = "embed-mihomo")]
fn cache_root() -> Result<PathBuf, Error> {
    if let Some(dir) = std::env::var_os("COCLASH_MIHOMO_DIR") {
        return Ok(PathBuf::from(dir));
    }
    dirs::cache_dir().ok_or_else(|| Error::Process("无法获取缓存目录".to_string()))
}

#[cfg(feature = "embed-mihomo")]
fn short_sha() -> &'static str {
    &SHA256[..8]
}

/// 纯函数：把 gz 释放到 `dir/mihomo[.exe]`；已存在且大小匹配则复用（可单测）。
#[cfg(feature = "embed-mihomo")]
pub fn ensure_extracted_with(gz: &[u8], dir: &Path, expected_size: u64) -> Result<PathBuf, Error> {
    let path = dir.join(exe_name());
    if fs::metadata(&path).is_ok_and(|m| m.len() == expected_size) {
        return Ok(path);
    }
    fs::create_dir_all(dir)?;
    let raw = decompress(gz)?;
    if raw.len() as u64 != expected_size {
        return Err(Error::Process(format!(
            "内嵌 mihomo 解压大小不符：期望 {expected_size}，实际 {}",
            raw.len()
        )));
    }
    let tmp = dir.join(format!("{}.tmp", exe_name()));
    fs::write(&tmp, &raw)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&tmp, fs::Permissions::from_mode(0o755))?;
    }
    if path.exists() {
        fs::remove_file(&path)?;
    }
    fs::rename(&tmp, &path)?;
    Ok(path)
}

/// 释放后的可执行文件名（保持规范名：Windows 镜像名匹配与日志都按它）
#[cfg(feature = "embed-mihomo")]
fn exe_name() -> &'static str {
    if cfg!(windows) {
        "mihomo.exe"
    } else {
        "mihomo"
    }
}

#[cfg(all(test, not(feature = "embed-mihomo")))]
mod stub_tests {
    #[test]
    fn test_stub_reports_not_embedded() {
        let err = super::ensure_extracted().unwrap_err().to_string();
        assert!(err.contains("未嵌入"), "stub 报错应说明未嵌入: {err}");
    }
}

#[cfg(all(test, feature = "embed-mihomo"))]
mod tests {
    use super::*;
    use std::io::Write;

    fn gz_of(raw: &[u8]) -> Vec<u8> {
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        encoder.write_all(raw).unwrap();
        encoder.finish().unwrap()
    }

    #[test]
    fn test_extract_and_reuse() {
        let dir = tempfile::TempDir::new().unwrap();
        let raw = b"fake-mihomo-binary".to_vec();
        let gz = gz_of(&raw);
        let path = ensure_extracted_with(&gz, dir.path(), raw.len() as u64).unwrap();
        assert_eq!(fs::read(&path).unwrap(), raw);
        assert_eq!(path.file_name().unwrap(), exe_name());

        // 篡改内容但保持大小：再次调用应命中缓存、不重写
        let tampered = vec![0u8; raw.len()];
        fs::write(&path, &tampered).unwrap();
        let again = ensure_extracted_with(&gz, dir.path(), raw.len() as u64).unwrap();
        assert_eq!(again, path);
        assert_eq!(fs::read(&path).unwrap(), tampered, "大小匹配即复用，不重写");
    }

    #[test]
    fn test_rewrites_when_size_mismatch() {
        let dir = tempfile::TempDir::new().unwrap();
        let raw = b"fake-mihomo-binary".to_vec();
        let gz = gz_of(&raw);
        fs::write(dir.path().join(exe_name()), b"truncated").unwrap();
        let out = ensure_extracted_with(&gz, dir.path(), raw.len() as u64).unwrap();
        assert_eq!(fs::read(&out).unwrap(), raw);
    }

    #[test]
    fn test_bad_gz_errors() {
        let dir = tempfile::TempDir::new().unwrap();
        assert!(ensure_extracted_with(b"not-gzip", dir.path(), 10).is_err());
        assert!(decompress(b"not-gzip").is_err());
    }

    #[test]
    fn test_decompress_embedded_matches_size() {
        let raw = decompress_embedded().unwrap();
        assert_eq!(
            raw.len() as u64,
            SIZE.parse::<u64>().unwrap(),
            "内存解压大小应与构建期记录一致"
        );
    }

    #[cfg(unix)]
    #[test]
    fn test_unix_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::TempDir::new().unwrap();
        let raw = b"x".to_vec();
        let gz = gz_of(&raw);
        let path = ensure_extracted_with(&gz, dir.path(), 1).unwrap();
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o755
        );
    }
}
