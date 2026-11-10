//! 词元（`Lexeme`）与轻量引用（`LexemeRef`）。
//!
//! # 对齐基准
//!
//! 对应 analysis-ik `core/src/main/java/org/wltea/analyzer/core/Lexeme.java`
//! （`master@6d2d70f`）。与 u6（`Lexeme.java`）的关系：Hash/Ord/PartialEq
//! 语义逐位一致（`absBegin*37 + absEnd*31 + (absBegin*absEnd)%len*11`）；
//! B 新增 `getLexemeTypeString()`（B:212-251，供 Lucene `TypeAttribute`），
//! 本库类型即 `i32` 常量、无 Lucene 层，不设对应物。
//! Rust 侧新增（无 Java 对应）：`LexemeRef`（全 `i32`、`Copy`，消除回溯
//! clone）、`Span` / `char_to_byte_map`（零拷贝入口）。

use std::cmp::Ordering;
use std::fmt;
use std::hash::{Hash, Hasher};

/// 词元的"轻量引用"表示（方案C）：仅含定位与类型字段，全部为 i32，可 `Copy`。
/// 分词过程（QuickSortSet / LexemePath / arbitrate）内部使用它，消除 `Lexeme` 携带
/// `Option<String>` 时的 clone 成本（回溯 clone 变为按位 Copy，零分配）。
#[derive(Debug, Clone, Copy)]
pub struct LexemeRef {
    // 词元的起始位移
    offset: i32,
    // 词元的相对起始位置
    begin: i32,
    // 词元的长度
    length: i32,
    // 词元类型
    lexeme_type: i32,
}

/// 方案C：LexemeRef 的相等/哈希仅比较定位字段（offset/begin/length），忽略 lexeme_type，
/// 与原 `Lexeme` 的 `PartialEq`/`Hash` 语义完全一致（Java 侧也比较 begin/end/length）。
impl PartialEq for LexemeRef {
    fn eq(&self, other: &Self) -> bool {
        self.offset == other.offset && self.begin == other.begin && self.length == other.length
    }
}
impl Eq for LexemeRef {}

impl Hash for LexemeRef {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // 用 i64 计算中间量，再 as i32 截断——与 Java 的 int 溢出语义一致，
        // 但避免 Rust debug 构建的 overflow panic / release 绕回（评审 P0-6）：
        // 绝对位置超过 ~46341 时 `abs_begin * abs_end` 在 i32 上溢出。
        let abs_begin = self.begin_position() as i64;
        let abs_end = self.end_position() as i64;
        let len = self.length as i64;
        let length_hash = if len > 0 {
            ((abs_begin * abs_end) % len) * 11
        } else {
            0
        };
        let hash_code: i32 = ((abs_begin * 37) + (abs_end * 31) + length_hash) as i32;
        hash_code.hash(state);
    }
}

impl LexemeRef {
    // --- lexemeType 常量 ---
    pub const TYPE_UNKNOWN: i32 = 0;
    pub const TYPE_ENGLISH: i32 = 1;
    pub const TYPE_ARABIC: i32 = 2;
    pub const TYPE_LETTER: i32 = 3;
    pub const TYPE_CNWORD: i32 = 4;
    pub const TYPE_CNCHAR: i32 = 64;
    pub const TYPE_OTHER_CJK: i32 = 8;
    pub const TYPE_CNUM: i32 = 16;
    pub const TYPE_COUNT: i32 = 32;
    pub const TYPE_CQUAN: i32 = 48;

    pub fn new(offset: i32, begin: i32, length: i32, lexeme_type: i32) -> Self {
        debug_assert!(length >= 0, "length < 0");
        Self {
            offset,
            begin,
            length,
            lexeme_type,
        }
    }

    pub fn offset(&self) -> i32 {
        self.offset
    }
    pub fn set_offset(&mut self, offset: i32) {
        self.offset = offset;
    }
    pub fn begin(&self) -> i32 {
        self.begin
    }
    pub fn set_begin(&mut self, begin: i32) {
        self.begin = begin;
    }
    pub fn begin_position(&self) -> i32 {
        self.offset + self.begin
    }
    pub fn end_position(&self) -> i32 {
        self.offset + self.begin + self.length
    }
    pub fn length(&self) -> i32 {
        self.length
    }
    pub fn set_length(&mut self, length: i32) {
        debug_assert!(length >= 0, "length < 0");
        self.length = length;
    }
    pub fn lexeme_type(&self) -> i32 {
        self.lexeme_type
    }
    pub fn set_lexeme_type(&mut self, lexeme_type: i32) {
        self.lexeme_type = lexeme_type;
    }

    /// 合并两个相邻的词元
    pub fn append(&mut self, other: &LexemeRef, lexeme_type: i32) -> bool {
        if self.end_position() == other.begin_position() {
            self.length += other.length();
            self.lexeme_type = lexeme_type;
            true
        } else {
            false
        }
    }
}

// --- 零拷贝位置区间（评审 P2-1）---

/// 词元的**位置区间**视图：只含定位与类型，**不持有文本**。
///
/// 由 [`crate::TokenizerFactory::segment_spans`] 产出。与 [`Lexeme`] 的区别：
/// `Lexeme` 在最终输出时为每个词元做一次 `String` 分配（实测该分配占分词热路径
/// 全部堆分配的 **94%**，见 `examples/alloc_count.rs`），而 `Span` 完全不分配文本，
/// 调用方用自己的原文 + 偏移切片即可还原词元。
///
/// 坐标语义与 [`Lexeme`] 一致：`begin()`/`end()` 是**全局字符**（`char`）下标，
/// 跨缓冲页连续；对 `&str` 切片需配合 [`char_to_byte_map`] 转换。
///
/// # 与 `Lexeme::text()` 的语义差异（重要）
///
/// [`Span::slice`] 返回的是**原文**子串，而 [`Lexeme::text()`] 返回**归一化后**
/// 的文本（输入侧 `regularize`：全角→半角，且 `enable_lowercase` 时 ASCII 小写化）。
/// 故对输入 `"IKAnalyzer2012"`：
///
/// ```
/// use ik_analyzer::{Config, TokenizerFactory, char_to_byte_map};
/// let factory = TokenizerFactory::new(Config::builder().build());
/// let text = "IKAnalyzer2012";
/// let map = char_to_byte_map(text);
/// let s = factory.segment_spans(text).next().unwrap();
/// assert_eq!(s.slice(text, &map), "IKAnalyzer2012"); // 原文
/// assert_eq!(factory.segmenter(text).next().unwrap().text(), "ikanalyzer2012"); // 归一化后
/// ```
///
/// **词元的位置与类型完全一致**，仅文本内容可能因归一化而不同。
/// 若下游需要归一化后的文本，请用 [`crate::TokenizerFactory::segmenter`]。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    /// 全局起始字符下标（含）
    begin: usize,
    /// 全局结束字符下标（不含）
    end: usize,
    /// 词元类型（与 [`Lexeme::lexeme_type`] 同口径）
    lexeme_type: i32,
}

impl Span {
    /// 由词元构造区间视图（丢弃文本）。
    pub(crate) fn from_lexeme(lexeme: &Lexeme) -> Self {
        Self {
            begin: lexeme.begin_position() as usize,
            end: lexeme.end_position() as usize,
            lexeme_type: lexeme.lexeme_type(),
        }
    }

    /// 全局起始字符下标（含）。
    pub fn begin(&self) -> usize {
        self.begin
    }

    /// 全局结束字符下标（不含）。
    pub fn end(&self) -> usize {
        self.end
    }

    /// 词元字符长度。
    pub fn length(&self) -> usize {
        self.end - self.begin
    }

    /// 词元类型（`Lexeme::TYPE_*` 常量）。
    pub fn lexeme_type(&self) -> i32 {
        self.lexeme_type
    }

    /// 用预先构建的 [`char_to_byte_map`] 从原文切出该区间，**零分配**。
    ///
    /// `char_to_byte` 必须是 `char_to_byte_map(source)` 的返回值（长度
    /// `字符数 + 1`）。整篇文本只需构建一次，即可供该文本的**全部** span 复用。
    ///
    /// # Panics
    ///
    /// `char_to_byte` 与 `source` 不匹配（长度不符或下标越界）时 panic。
    pub fn slice<'a>(&self, source: &'a str, char_to_byte: &[u32]) -> &'a str {
        assert!(
            self.end < char_to_byte.len(),
            "char_to_byte 长度 {} 不足以索引 span 终点 {}",
            char_to_byte.len(),
            self.end
        );
        let b = char_to_byte[self.begin] as usize;
        let e = char_to_byte[self.end] as usize;
        &source[b..e]
    }
}

/// 构建 `char` 下标 → `byte` 偏移的映射表，供 [`Span::slice`] 使用。
///
/// 返回长度为 `字符数 + 1` 的向量：第 `i` 项是第 `i` 个字符的起始字节偏移，
/// **末项为 `source.len()`**（即最后一个字符之后的位置，便于切片到结尾）。
///
/// 用 `u32` 而非 `usize` 以省一半内存；4 GiB 以内的文本足够（超出请自行分段）。
///
/// ```
/// use ik_analyzer::char_to_byte_map;
/// let text = "中文abc";                       // 5 个字符
/// let m = char_to_byte_map(text);
/// assert_eq!(m.len(), text.chars().count() + 1); // 末项为 text.len()
/// assert_eq!((m[0], m[2], m[5]), (0, 6, 9));
/// assert_eq!(m.last().copied().unwrap() as usize, text.len());
/// ```
pub fn char_to_byte_map(source: &str) -> Vec<u32> {
    let mut map: Vec<u32> = source
        .char_indices()
        .map(|(byte_index, _)| byte_index as u32)
        .collect();
    map.push(source.len() as u32);
    map
}

/// 实现 Ord 以支持排序（与原始 Lexeme 一致）
impl Ord for LexemeRef {
    fn cmp(&self, other: &Self) -> Ordering {
        self.begin
            .cmp(&other.begin)
            .then_with(|| other.length.cmp(&self.length))
    }
}
impl PartialOrd for LexemeRef {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// IK 词元对象（公开输出类型，行为与原版完全一致）
#[derive(Debug, Clone)]
pub struct Lexeme {
    inner: LexemeRef,
    // 词元文本：分词过程中为 None（零分配），最终输出时才填入
    text: Option<String>,
}

impl Lexeme {
    // --- lexemeType 常量（透传，保持公开 API 不变）---
    pub const TYPE_UNKNOWN: i32 = LexemeRef::TYPE_UNKNOWN;
    pub const TYPE_ENGLISH: i32 = LexemeRef::TYPE_ENGLISH;
    pub const TYPE_ARABIC: i32 = LexemeRef::TYPE_ARABIC;
    pub const TYPE_LETTER: i32 = LexemeRef::TYPE_LETTER;
    pub const TYPE_CNWORD: i32 = LexemeRef::TYPE_CNWORD;
    pub const TYPE_CNCHAR: i32 = LexemeRef::TYPE_CNCHAR;
    pub const TYPE_OTHER_CJK: i32 = LexemeRef::TYPE_OTHER_CJK;
    pub const TYPE_CNUM: i32 = LexemeRef::TYPE_CNUM;
    pub const TYPE_COUNT: i32 = LexemeRef::TYPE_COUNT;
    pub const TYPE_CQUAN: i32 = LexemeRef::TYPE_CQUAN;

    /// 由轻量引用构造（不带文本），供分词过程数据结构向公开类型转换
    pub(crate) fn from_ref(inner: LexemeRef) -> Self {
        Self { inner, text: None }
    }

    /// 获取内部轻量引用（供分词过程数据结构使用）。
    /// 改名 `to_ref`（原 `as_ref`）以避免与 `AsRef` trait 方法混淆——
    /// 它返回 owned `LexemeRef`，语义与 `AsRef::as_ref`（返回引用）相反（评审 P2-6）。
    pub(crate) fn to_ref(&self) -> LexemeRef {
        self.inner
    }

    /// 构造一个新的 Lexeme 实例（公开 API 保持兼容）。
    ///
    /// # 信任边界
    ///
    /// `offset` / `begin` 分别是全局位移与**页内**相对位置，`length` 为词元长度。
    /// 分词器只产出 `length >= 0` 的词元（`LexemeRef::new` 有 `debug_assert`），
    /// 但本构造器是 `pub`，**非受信入参不会被拒绝**：
    ///
    /// - `length < 0` 在 debug 构建下触发 `debug_assert`；release 下不触发。
    ///   `Hash` 实现有 `len > 0` 保护（安全），`Ord`/`Display` 亦不会 panic；
    ///   风险仅在于调用方随后按 `begin..begin+length` 切片时可能 panic。
    /// - `lexeme_type` 越界值不影响安全：`Display` 落到 `"INVALID_TYPE"` 分支。
    ///
    /// 非受信输入请先自行校验。本库内部词元一律满足 `length >= 0`，
    /// 且 `begin`/`length` 落在当前缓冲页内 —— `analyze_context::get_next_lexeme`
    /// 的文本取值与停用词查询都依赖这一点。
    pub fn new(offset: i32, begin: i32, length: i32, lexeme_type: i32) -> Self {
        Self {
            inner: LexemeRef::new(offset, begin, length, lexeme_type),
            text: None,
        }
    }

    // --- Getters and Setters（委托到 inner）---
    pub fn offset(&self) -> i32 {
        self.inner.offset()
    }
    pub fn set_offset(&mut self, offset: i32) {
        self.inner.set_offset(offset);
    }

    pub fn begin(&self) -> i32 {
        self.inner.begin()
    }
    pub fn set_begin(&mut self, begin: i32) {
        self.inner.set_begin(begin);
    }

    pub fn begin_position(&self) -> i32 {
        self.inner.begin_position()
    }
    pub fn end_position(&self) -> i32 {
        self.inner.end_position()
    }

    pub fn length(&self) -> i32 {
        self.inner.length()
    }
    pub fn set_length(&mut self, length: i32) {
        self.inner.set_length(length);
    }

    pub fn text(&self) -> &str {
        self.text.as_deref().unwrap_or("")
    }
    pub fn set_text(&mut self, text: String) {
        self.text = Some(text);
    }

    pub fn lexeme_type(&self) -> i32 {
        self.inner.lexeme_type()
    }
    pub fn set_lexeme_type(&mut self, lexeme_type: i32) {
        self.inner.set_lexeme_type(lexeme_type);
    }

    /// 合并两个相邻的词元
    pub fn append(&mut self, other: &Lexeme, lexeme_type: i32) -> bool {
        let oref = other.inner;
        let mut tmp = self.inner;
        let ok = tmp.append(&oref, lexeme_type);
        if ok {
            self.inner = tmp;
        }
        ok
    }
}

// --- Trait Implementations ---

/// 实现 PartialEq 以支持 `==` 和 `!=`（仅比较定位，与 Java 一致）
impl PartialEq for Lexeme {
    fn eq(&self, other: &Self) -> bool {
        self.inner == other.inner
    }
}
impl Eq for Lexeme {}

/// 实现 Hash 以支持在 HashMap/HashSet 中使用
impl Hash for Lexeme {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // 用 i64 计算中间量再 as i32 截断，与 Java 的 int 溢出语义一致，
        // 避免 Rust debug panic / release 绕回（评审 P0-6）。
        // Java: (absBegin * 37) + (absEnd * 31) + ((absBegin * absEnd) % getLength()) * 11
        let abs_begin = self.begin_position() as i64;
        let abs_end = self.end_position() as i64;
        let len = self.length() as i64;

        // 注意：如果 length 为 0，Java 的 `%` 会抛异常，Rust 会 panic。
        let length_hash = if len > 0 {
            ((abs_begin * abs_end) % len) * 11
        } else {
            0
        };

        let hash_code: i32 = ((abs_begin * 37) + (abs_end * 31) + length_hash) as i32;
        hash_code.hash(state);
    }
}

/// 实现 Ord 以支持排序（委托到 LexemeRef）
impl Ord for Lexeme {
    fn cmp(&self, other: &Self) -> Ordering {
        self.inner.cmp(&other.inner)
    }
}

impl PartialOrd for Lexeme {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// 实现 Display 以支持格式化输出 (toString)
impl fmt::Display for Lexeme {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let type_str = match self.lexeme_type() {
            Self::TYPE_UNKNOWN => "UNKNOWN",
            Self::TYPE_ENGLISH => "ENGLISH",
            Self::TYPE_ARABIC => "ARABIC",
            Self::TYPE_LETTER => "LETTER",
            Self::TYPE_CNWORD => "CN_WORD",
            Self::TYPE_CNCHAR => "CN_CHAR",
            Self::TYPE_OTHER_CJK => "OTHER_CJK",
            Self::TYPE_COUNT => "COUNT",
            Self::TYPE_CNUM => "CN_NUM",
            Self::TYPE_CQUAN => "CN_QUAN",
            _ => "INVALID_TYPE",
        };

        write!(
            f,
            "{}-{} : {} : \t{}",
            self.begin_position(),
            self.end_position(),
            self.text.as_deref().unwrap_or(""),
            type_str
        )
    }
}

// --- 单元测试 ---
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_equality_and_hash() {
        let l1 = Lexeme::new(10, 5, 2, Lexeme::TYPE_CNWORD);
        let l2 = Lexeme::new(10, 5, 2, Lexeme::TYPE_ENGLISH); // type different, but eq
        let l3 = Lexeme::new(10, 6, 2, Lexeme::TYPE_CNWORD); // begin different

        assert_eq!(l1, l2);
        assert_ne!(l1, l3);

        let mut map = std::collections::HashMap::new();
        map.insert(l1, "lexeme1");
        assert!(map.contains_key(&l2));
    }

    #[test]
    fn test_ordering() {
        let l1 = Lexeme::new(0, 5, 3, 0); // begin: 5, len: 3
        let l2 = Lexeme::new(0, 5, 4, 0); // begin: 5, len: 4 (longer, should be smaller)
        let l3 = Lexeme::new(0, 6, 3, 0); // begin: 6 (should be larger)

        assert_eq!(l2.cmp(&l1), Ordering::Less); // l2 < l1
        assert_eq!(l1.cmp(&l3), Ordering::Less); // l1 < l3

        let mut lexemes = [l3.clone(), l1.clone(), l2.clone()];
        lexemes.sort();

        // Expected order: l2, l1, l3
        assert_eq!(lexemes[0], l2);
        assert_eq!(lexemes[1], l1);
        assert_eq!(lexemes[2], l3);
    }

    #[test]
    fn test_append() {
        let mut l1 = Lexeme::new(0, 0, 2, Lexeme::TYPE_ARABIC); // text[0..2]
        let l2 = Lexeme::new(0, 2, 2, Lexeme::TYPE_COUNT); // text[2..4]

        assert!(l1.append(&l2, Lexeme::TYPE_CQUAN));
        assert_eq!(l1.length(), 4);
        assert_eq!(l1.lexeme_type(), Lexeme::TYPE_CQUAN);
        assert_eq!(l1.end_position(), 4);
    }

    #[test]
    fn test_display() {
        let mut lexeme = Lexeme::new(100, 5, 2, Lexeme::TYPE_CNWORD);
        lexeme.set_text("中国".to_string());

        let expected = "105-107 : 中国 : \tCN_WORD";
        assert_eq!(lexeme.text(), "中国");
        assert_eq!(lexeme.to_string(), expected);
    }
}
