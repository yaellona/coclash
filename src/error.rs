//! 类型化错误：按失败来源分类，UI 层统一 `Display` 格式化为日志文本。
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("{0}")]
    Api(String),
    #[error("{0}")]
    Process(String),
    #[error("{0}")]
    Config(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
}
