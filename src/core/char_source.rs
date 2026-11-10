//! 字符输入源抽象：分词核心只认此 trait，不关心数据来自内存还是流。
//!
//! # 对齐说明
//!
//! 无 Java 对应物——替代 Java `java.io.Reader` + `AnalyzeContext.fillBuffer(Reader)`：
//! u6/B 从 `Reader` 读到的已是解码好的 UTF-16 `char` 流，本库直接吃**字节流**，
//! 增量 UTF-8 解码、正确处理跨 buffer 的多字节字符切分与非法 UTF-8 → `U+FFFD`
//! 替换（两侧 Java 解码器同样产出 U+FFFD，行为一致）。Rust `char` 模型因此
//! 天然无 analysis-ik `SurrogatePairSegmenter` 的「跨页代理对拆半个」缺陷
//! （调研报告 §9.2.1）。

/// 字符输入源抽象：分词核心只认此 trait，不关心数据来自内存还是流。
///
/// 两个适配器：
/// - [`StrCharSource`]：内存 `&str`，知道总长，可按 `min(len, 4096)` 分配缓冲
/// - [`ReadCharSource`]：`impl Read`，增量解码 UTF-8，恒定内存
pub trait CharSource: Send {
    /// 将字符填入 buf，返回填入数量。返回 0 表示 EOF。
    fn fill(&mut self, buf: &mut [char]) -> usize;

    /// 如果已知总字符数，返回 `Some(len)`。用于短输入按 `min(len, BUFF_SIZE)` 分配缓冲。
    fn len_hint(&self) -> Option<usize> {
        None
    }

    /// 取走并返回上一次 `fill` 期间记录的 IO 错误（若有）。
    ///
    /// 默认返回 `None`（无错误信息可上报）。`ReadCharSource` 会把读取错误记录到
    /// 内部字段供上层在 EOF 时感知——避免"流式分词成功返回但内容被静默截断"
    /// 的问题（评审 P0-5）。取走后内部记录清空。
    ///
    /// 注意：`IKSegmenter` 在迭代结束时调用本方法，并把结果转存到
    /// [`crate::IKSegmenter::take_error`]，因此自定义 `CharSource` 实现者
    /// 无需自行缓存错误。
    fn take_io_error(&mut self) -> Option<std::io::Error> {
        None
    }
}

// ---------------------------------------------------------------------------
// 内存适配器
// ---------------------------------------------------------------------------

/// 从 `&str` 构造的内存字符源。
///
/// 持有 `String`（按需解码），不再一次性 `collect::<Vec<char>>`——对纯 ASCII
/// 输入省去 4 倍内存（评审 P1-10）。`fill` 时按 `char_indices` 增量推进，
/// 每次最多解码到 buf 填满。
///
/// # 内存特征（注意）
///
/// 本适配器仍会对输入做**一次全量`String` 拷贝**（`str` → `String`），
/// 且 `len_hint()` 会做一次全量 `chars().count()` 扫描。因此：
///
/// - [`crate::TokenizerFactory::segmenter`]（内存路径）：拷贝 1 份输入，
///   **不属于**"恒定内存"；
/// - [`crate::TokenizerFactory::segmenter_from_read`]（流式路径）：
///   真正的恒定内存，不随输入总长增长。
///
/// 之所以仍保留拷贝而非借用 `&str`：分词器需拥有数据以延后解码（跨页时
/// `fill_buffer` 要保留上一页尾部字符），且 `CharSource: Send` 要求 `'static`。
/// 若需彻底消除拷贝，可另开一个借用型入口（属 breaking change，另行评估）。
pub struct StrCharSource {
    text: String,
    /// 下一个待解码字节的字节偏移
    byte_pos: usize,
}

impl StrCharSource {
    pub fn new(s: &str) -> Self {
        Self {
            text: s.to_string(),
            byte_pos: 0,
        }
    }
}

impl CharSource for StrCharSource {
    fn fill(&mut self, buf: &mut [char]) -> usize {
        let mut filled = 0;
        while filled < buf.len() && self.byte_pos < self.text.len() {
            // 安全：str 保证合法 UTF-8，从任意字节边界解码一个 char
            let rest = &self.text[self.byte_pos..];
            let c = rest.chars().next().expect("str 非空且为合法 UTF-8");
            buf[filled] = c;
            filled += 1;
            self.byte_pos += c.len_utf8();
        }
        filled
    }

    fn len_hint(&self) -> Option<usize> {
        if self.byte_pos >= self.text.len() {
            Some(0)
        } else {
            Some(self.text[self.byte_pos..].chars().count())
        }
    }

    fn take_io_error(&mut self) -> Option<std::io::Error> {
        None
    }
}

// ---------------------------------------------------------------------------
// 流式适配器
// ---------------------------------------------------------------------------

/// 从 `impl Read` 构造的流式字符源。
/// 增量解码 UTF-8，正确处理跨 read 边界的半截字符。
/// 无效 UTF-8 序列替换为 U+FFFD。
///
/// IO 错误（断流、权限、I/O 中断）不再被静默当作 EOF：错误被记录到
/// `last_error` 并通过 `take_io_error` 上报，由 `IKSegmenter` 在结束时
/// 以 `log::error` 透出（评审 P0-5）。
pub struct ReadCharSource<R: std::io::Read + Send> {
    reader: R,
    /// 上次 read 残留的未完成 UTF-8 字节
    incomplete: Vec<u8>,
    /// 已解码但尚未被 fill 消费的字符队列
    pending: std::collections::VecDeque<char>,
    /// 上一次 fill 期间记录的 IO 错误（供 take_io_error 取走）
    last_error: Option<std::io::Error>,
}

impl<R: std::io::Read + Send> ReadCharSource<R> {
    pub fn new(reader: R) -> Self {
        Self {
            reader,
            incomplete: Vec::with_capacity(8),
            pending: std::collections::VecDeque::new(),
            last_error: None,
        }
    }

    /// 尝试解码 incomplete 中的字节，将完整字符推入 pending。
    /// 返回是否还有未完成的序列（需要更多字节）。
    /// 迭代循环实现，消除递归（评审 P0-5）。
    fn try_decode(&mut self) -> bool {
        loop {
            if self.incomplete.is_empty() {
                return false;
            }

            match std::str::from_utf8(&self.incomplete) {
                Ok(s) => {
                    // 全部解码成功
                    self.pending.extend(s.chars());
                    self.incomplete.clear();
                    return false;
                }
                Err(e) => {
                    let valid_len = e.valid_up_to();
                    if valid_len > 0 {
                        // 解码有效前缀
                        let s = std::str::from_utf8(&self.incomplete[..valid_len]).unwrap();
                        self.pending.extend(s.chars());
                        self.incomplete = self.incomplete[valid_len..].to_vec();
                        // 继续循环处理剩余
                        continue;
                    }
                    // valid_len == 0：0 位置是无效或不可完成的序列
                    if let Some(error_len) = e.error_len() {
                        // 确定无效的子序列：整段替换为单个 U+FFFD，与
                        // `String::from_utf8_lossy` 的"每个非法子序列一个 U+FFFD"
                        // 语义对齐（评审 P0-5）。
                        let _ = error_len; // 仅用于 drain；FFFD 个数固定为 1
                        self.pending.push_back('\u{FFFD}');
                        self.incomplete.drain(..error_len);
                        // 继续循环处理剩余
                        continue;
                    }
                    // error_len == None：序列不完整，需要更多字节
                    return true;
                }
            }
        }
    }
}

impl<R: std::io::Read + Send> CharSource for ReadCharSource<R> {
    fn fill(&mut self, buf: &mut [char]) -> usize {
        let mut filled = 0;
        let mut byte_buf = [0u8; 8192]; // 栈分配，避免堆

        loop {
            // 1. 消费 pending 中的已解码字符
            while filled < buf.len() {
                if let Some(c) = self.pending.pop_front() {
                    buf[filled] = c;
                    filled += 1;
                } else {
                    break;
                }
            }
            if filled >= buf.len() {
                break;
            }

            // 2. 尝试解码残留字节
            self.try_decode();

            // 消费新解码的字符
            while filled < buf.len() {
                if let Some(c) = self.pending.pop_front() {
                    buf[filled] = c;
                    filled += 1;
                } else {
                    break;
                }
            }
            if filled >= buf.len() {
                break;
            }

            // 3. 如果还有不完整序列且没有 pending，需要读更多字节
            //    如果没有不完整序列，也继续读
            let n = match self.reader.read(&mut byte_buf) {
                Ok(0) => {
                    // EOF：冲洗残留的不完整字节为替换字符
                    if !self.incomplete.is_empty() {
                        self.pending.push_back('\u{FFFD}');
                        self.incomplete.clear();
                        continue;
                    }
                    break;
                }
                Ok(n) => n,
                Err(e) => {
                    // IO 错误不再静默当作 EOF：记录到 last_error 供上层感知，
                    // 同时停止读取（当作 EOF 结束本次 fill，已解码字符仍被返回）。
                    self.last_error = Some(e);
                    break;
                }
            };

            self.incomplete.extend_from_slice(&byte_buf[..n]);
            // 下一轮循环会 try_decode + 消费
        }

        filled
    }

    fn take_io_error(&mut self) -> Option<std::io::Error> {
        self.last_error.take()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn test_str_source_short() {
        let mut src = StrCharSource::new("你好");
        assert_eq!(src.len_hint(), Some(2));
        let mut buf = [0 as char; 10];
        let n = src.fill(&mut buf);
        assert_eq!(n, 2);
        assert_eq!(buf[0], '你');
        assert_eq!(buf[1], '好');
        // 再 fill 应返回 0（EOF）
        assert_eq!(src.fill(&mut buf), 0);
    }

    #[test]
    fn test_str_source_long() {
        let s = "a".repeat(5000);
        let mut src = StrCharSource::new(&s);
        assert_eq!(src.len_hint(), Some(5000));
        let mut buf = [0 as char; 4096];
        let n1 = src.fill(&mut buf);
        assert_eq!(n1, 4096);
        let n2 = src.fill(&mut buf);
        assert_eq!(n2, 904);
        assert_eq!(src.fill(&mut buf), 0);
    }

    #[test]
    fn test_read_source_basic() {
        let data = "你好世界".as_bytes().to_vec();
        let mut src = ReadCharSource::new(Cursor::new(data));
        let mut buf = [0 as char; 10];
        let n = src.fill(&mut buf);
        assert_eq!(n, 4);
        assert_eq!(buf[0], '你');
        assert_eq!(buf[3], '界');
    }

    #[test]
    fn test_read_source_cross_boundary() {
        // 构造跨 read 边界的 UTF-8
        let s = "你好世界".repeat(100);
        let bytes = s.into_bytes();
        // 分两次读取，每次 100 字节，可能切断 UTF-8 序列
        let mut src = ReadCharSource::new(Cursor::new(bytes));
        let mut all = Vec::new();
        let mut buf = [0 as char; 50];
        loop {
            let n = src.fill(&mut buf);
            if n == 0 {
                break;
            }
            all.extend_from_slice(&buf[..n]);
        }
        assert_eq!(all.len(), 400);
        assert_eq!(all[0], '你');
        assert_eq!(all[399], '界');
    }

    #[test]
    fn test_read_source_invalid_utf8() {
        // 0xFF 是无效 UTF-8
        let data = vec![0xFF, 0xFF, b'h', b'i'];
        let mut src = ReadCharSource::new(Cursor::new(data));
        let mut buf = [0 as char; 10];
        let n = src.fill(&mut buf);
        assert_eq!(n, 4); // 2 个替换 + 2 个 ASCII
        assert_eq!(buf[0], '\u{FFFD}');
        assert_eq!(buf[1], '\u{FFFD}');
        assert_eq!(buf[2], 'h');
        assert_eq!(buf[3], 'i');
    }
}
