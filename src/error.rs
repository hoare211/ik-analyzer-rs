//! 错误类型。
//!
//! # 对齐说明
//!
//! 无 Java 对应物：u6/B 词典加载失败抛 `RuntimeException` 或静默吞掉，
//! 本库以 `IkResult` 区分 `Io / FileNotFound / InvalidEncoding`
//! （词典少词/坏行可被上层感知，而非静默）。

use std::fmt;
use std::io;

/// ik-analyzer-rs 错误类型
#[derive(Debug)]
pub enum IkError {
    /// 词典文件 IO 错误
    Io(io::Error),
    /// 词典文件不存在
    FileNotFound(String),
    /// 词典行解码错误（非 UTF-8）。携带底层解码错误以便追溯。
    InvalidEncoding {
        path: String,
        line: usize,
        source: Box<dyn std::error::Error + Send + Sync + 'static>,
    },
}

impl fmt::Display for IkError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            IkError::Io(e) => write!(f, "IO error: {}", e),
            IkError::FileNotFound(path) => write!(f, "dictionary file not found: {}", path),
            IkError::InvalidEncoding { path, line, source } => {
                write!(
                    f,
                    "invalid UTF-8 in dictionary {} at line {}: {}",
                    path, line, source
                )
            }
        }
    }
}

impl std::error::Error for IkError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            IkError::Io(e) => Some(e),
            IkError::InvalidEncoding { source, .. } => Some(source.as_ref()),
            _ => None,
        }
    }
}

// 不在此处对 `io::Error` 做特殊处理：
// `From<io::Error>` 拿不到触发 IO 的文件路径，把 `NotFound` 特判为 `FileNotFound(String::new())`
// 会让错误信息变成 `dictionary file not found: `（路径为空），对排障毫无帮助。
// 调用方（如 `build_dawg`）应在打开文件失败时显式把路径塞进错误消息再转成 `IkError::Io`。
// `FileNotFound` 变体保留用于语义明确的"词典文件不存在"上层判断（携带真实路径）。
impl From<io::Error> for IkError {
    fn from(e: io::Error) -> Self {
        IkError::Io(e)
    }
}

pub type IkResult<T> = Result<T, IkError>;
