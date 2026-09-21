//! 构建期：把 mihomo 压缩嵌入 coclash。
//!
//! 取源顺序（仅在启用 `embed-mihomo` feature 时执行，否则不联网不产出）：
//! 1. `COCLASH_EMBED_MIHOMO`（构建期注入：原始可执行文件或 `.gz`/`.zip` 归档；Nix/离线用）
//! 2. `vendor/mihomo/<target>/mihomo[.exe]`（本地优先，直接复用）
//! 3. 按 `mihomo.lock` 下载官方发布资产，校验 sha256 后解压落盘到 vendor 目录
//!
//! 产出：`$OUT_DIR/mihomo.gz` + `coclash_embed_mihomo` cfg 与
//! `COCLASH_EMBEDDED_MIHOMO_VERSION/SHA256/SIZE` 编译期常量。
use std::env;
use std::fs;
use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};

const LOCK_FILE: &str = "mihomo.lock";
const VENDOR_DIR: &str = "vendor/mihomo";

fn main() {
    println!("cargo:rerun-if-changed={LOCK_FILE}");
    println!("cargo:rerun-if-env-changed=COCLASH_EMBED_MIHOMO");
    println!("cargo:rerun-if-changed={VENDOR_DIR}");

    if env::var_os("CARGO_FEATURE_EMBED_MIHOMO").is_none() {
        return;
    }

    let target = env::var("TARGET").expect("缺少 TARGET");
    let manifest_dir =
        PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("缺少 CARGO_MANIFEST_DIR"));
    let lock = Lock::load(&manifest_dir.join(LOCK_FILE));
    let spec = lock.spec(&target).unwrap_or_else(|| {
        panic!(
            "mihomo.lock 中没有目标平台 `{target}` 的资产；\
             请补充 lock，或用 COCLASH_EMBED_MIHOMO 指定可执行文件"
        )
    });

    let exe_name = if target.contains("windows") {
        "mihomo.exe"
    } else {
        "mihomo"
    };
    let vendor_path = manifest_dir.join(VENDOR_DIR).join(&target).join(exe_name);

    let raw = if let Some(path) = env::var_os("COCLASH_EMBED_MIHOMO") {
        read_source(Path::new(&path))
            .unwrap_or_else(|e| panic!("读取 COCLASH_EMBED_MIHOMO 失败: {e}"))
    } else if vendor_path.is_file() {
        fs::read(&vendor_path)
            .unwrap_or_else(|e| panic!("读取 {} 失败: {e}", vendor_path.display()))
    } else {
        let raw = download_and_extract(&lock.version, &spec).unwrap_or_else(|e| panic!("{e}"));
        if let Some(parent) = vendor_path.parent() {
            fs::create_dir_all(parent)
                .unwrap_or_else(|e| panic!("创建 {} 失败: {e}", parent.display()));
        }
        fs::write(&vendor_path, &raw)
            .unwrap_or_else(|e| panic!("写入 {} 失败: {e}", vendor_path.display()));
        println!(
            "cargo:warning=已下载 mihomo {} 到 {}",
            lock.version,
            vendor_path.display()
        );
        raw
    };

    let sha256 = sha256_hex(&raw);
    let size = raw.len();
    let out_path = PathBuf::from(env::var("OUT_DIR").expect("缺少 OUT_DIR")).join("mihomo.gz");
    write_gz(&out_path, &raw);

    println!("cargo:rustc-cfg=coclash_embed_mihomo");
    println!(
        "cargo:rustc-env=COCLASH_EMBEDDED_MIHOMO_VERSION={}",
        lock.version
    );
    println!("cargo:rustc-env=COCLASH_EMBEDDED_MIHOMO_SHA256={sha256}");
    println!("cargo:rustc-env=COCLASH_EMBEDDED_MIHOMO_SIZE={size}");
}

/// `mihomo.lock`：固定版本 + 各 target 的资产名与归档 sha256
struct Lock {
    version: String,
    targets: toml::value::Table,
}

struct AssetSpec {
    asset: String,
    archive_sha256: String,
}

impl Lock {
    fn load(path: &Path) -> Self {
        let text = fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("读取 {} 失败: {e}", path.display()));
        let value: toml::Value =
            toml::from_str(&text).unwrap_or_else(|e| panic!("解析 {} 失败: {e}", path.display()));
        let version = value
            .get("version")
            .and_then(|v| v.as_str())
            .expect("mihomo.lock 缺少 version")
            .to_string();
        let targets = value
            .get("targets")
            .and_then(|v| v.as_table())
            .cloned()
            .unwrap_or_default();
        Self { version, targets }
    }

    fn spec(&self, target: &str) -> Option<AssetSpec> {
        let entry = self.targets.get(target)?.as_table()?;
        Some(AssetSpec {
            asset: entry.get("asset")?.as_str()?.to_string(),
            archive_sha256: entry.get("archive_sha256")?.as_str()?.to_string(),
        })
    }
}

/// 读取构建期注入的源：按扩展名识别 `.gz`/`.zip`，否则视为原始可执行文件
fn read_source(path: &Path) -> Result<Vec<u8>, String> {
    let bytes = fs::read(path).map_err(|e| format!("读取 {} 失败: {e}", path.display()))?;
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    extract_asset(name, bytes)
}

/// 下载官方资产并校验 sha256，返回解压后的可执行文件（失败重试 3 次）
fn download_and_extract(version: &str, spec: &AssetSpec) -> Result<Vec<u8>, String> {
    use std::time::Duration;

    let url = format!(
        "https://github.com/MetaCubeX/mihomo/releases/download/{version}/{}",
        spec.asset
    );
    let agent = ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(30))
        .timeout_read(Duration::from_secs(600))
        .build();
    let mut last_err = String::new();
    for attempt in 1..=3 {
        eprintln!("coclash build: 下载 {url}（第 {attempt} 次）");
        match agent.get(&url).call() {
            Ok(resp) => {
                let expected = resp
                    .header("Content-Length")
                    .and_then(|v| v.parse::<usize>().ok());
                let mut archive = Vec::new();
                match resp.into_reader().read_to_end(&mut archive) {
                    Ok(_) if expected.is_none_or(|n| n == archive.len()) => {
                        let got = sha256_hex(&archive);
                        if !got.eq_ignore_ascii_case(&spec.archive_sha256) {
                            return Err(format!(
                                "{} 校验失败: 期望 {}，实际 {got}",
                                spec.asset, spec.archive_sha256
                            ));
                        }
                        return extract_asset(&spec.asset, archive);
                    }
                    Ok(_) => {
                        last_err = format!(
                            "下载不完整（期望 {expected:?} 字节，实际 {}）",
                            archive.len()
                        );
                    }
                    Err(e) => last_err = format!("读取下载内容失败: {e}"),
                }
            }
            Err(e) => last_err = format!("下载失败: {e}"),
        }
        std::thread::sleep(Duration::from_secs(1));
    }
    Err(format!("下载 {url} 失败（已重试 3 次）: {last_err}"))
}

/// 按归档名解压：`.gz` → flate2；`.zip` → 取第一个 `.exe`；其他视为原始文件
fn extract_asset(name: &str, bytes: Vec<u8>) -> Result<Vec<u8>, String> {
    let lower = name.to_ascii_lowercase();
    if lower.ends_with(".gz") {
        gunzip(&bytes)
    } else if lower.ends_with(".zip") {
        unzip_exe(&bytes)
    } else {
        Ok(bytes)
    }
}

fn gunzip(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    flate2::read::GzDecoder::new(Cursor::new(bytes))
        .read_to_end(&mut out)
        .map_err(|e| format!("解压 gz 失败: {e}"))?;
    Ok(out)
}

fn unzip_exe(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let mut archive =
        zip::ZipArchive::new(Cursor::new(bytes)).map_err(|e| format!("读取 zip 失败: {e}"))?;
    for i in 0..archive.len() {
        let mut file = archive
            .by_index(i)
            .map_err(|e| format!("读取 zip 条目失败: {e}"))?;
        if file.name().to_ascii_lowercase().ends_with(".exe") {
            let mut out = Vec::new();
            file.read_to_end(&mut out)
                .map_err(|e| format!("解压 zip 条目失败: {e}"))?;
            return Ok(out);
        }
    }
    Err("zip 中未找到 .exe".to_string())
}

fn write_gz(path: &Path, raw: &[u8]) {
    let file =
        fs::File::create(path).unwrap_or_else(|e| panic!("创建 {} 失败: {e}", path.display()));
    let mut encoder = flate2::write::GzEncoder::new(file, flate2::Compression::best());
    encoder
        .write_all(raw)
        .unwrap_or_else(|e| panic!("写入 {} 失败: {e}", path.display()));
    encoder
        .finish()
        .unwrap_or_else(|e| panic!("完成 {} 失败: {e}", path.display()));
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}
