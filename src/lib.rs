//! # ik-analyzer-rs
//!
//! A high-performance, thread-safe Rust Chinese word segmentation library from the
//! [IK Analyzer](https://code.google.com/p/ik-analyzer/) lineage.
//!
//! The implementation baseline is analysis-ik rather than the 2012_u6 source:
//! compatibility with u6 is an explicit, opt-in contract (`Config::u6_parity`),
//! not the default behavior. Defaults are independent product decisions and may
//! improve on the original.
//!
//! ## Quick Start
//!
//! ```
//! use ik_analyzer::{Config, TokenizerFactory};
//!
//! let factory = TokenizerFactory::new(Config::builder().use_smart(true).build());
//! let mut seg = factory.segmenter("欢迎使用本分词器");
//! assert_eq!(seg.next().unwrap().text(), "欢迎");
//! assert_eq!(seg.next().unwrap().text(), "使用");
//! ```
//!
//! ## Architecture
//!
//! - [`TokenizerFactory`] loads dictionaries once and creates lightweight [`IKSegmenter`] instances.
//! - [`IKSegmenter`] implements `Iterator<Item = Lexeme>`.
//! - [`Config`] / [`ConfigBuilder`] for customization.

// 兑现 README "零 unsafe" 承诺：全局禁止 unsafe（评审 P4-3）。
#![forbid(unsafe_code)]

use std::sync::Arc;

pub mod cfg;
pub mod error;

/// 本 crate 的版本号（取自 `Cargo.toml`，经 `CARGO_PKG_VERSION` 注入）。
///
/// # 为什么需要它
///
/// Python 绑定的 `ik_analyzer.__version__` 过去取的是 **python crate** 的
/// `CARGO_PKG_VERSION`，而实际链接进`.so` 的是核心库 —— 两者一度漂移
/// （python 1.1.1 / 核心 1.1.2），导致 Python 用户无法判断自己拿到的是哪个
/// 核心库版本（评审 P3-1）。绑定层应改为引用本常量。
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

// `core` / `dic` 收敛为私有模块（评审 P3-3）：避免把 analyze_context / dawg_builder /
// hit / persistent_hit 等纯内部结构摊到公共 API 与 docs.rs。仅通过下方 `pub use`
// 暴露稳定公共类型，后续内部重命名不再是 breaking change。
mod core;
mod dic;

// --- 导出新的公共 API ---
pub use cfg::{AstralMode, Config, ConfigBuilder, DictSource};
pub use core::{
    CharSource, IKSegmenter, Lexeme, ReadCharSource, Span, SpanSegmenter, StrCharSource,
    char_to_byte_map,
};
pub use dic::{Dictionary, Hit};
pub use error::{IkError, IkResult};

/// 分词器工厂
///
/// 负责一次性加载词典，并创建共享这些词典的分词器实例。
/// 这是使用本库的推荐入口点。
#[derive(Clone)]
pub struct TokenizerFactory {
    dictionary: Arc<Dictionary>,
    /// 改为 `Arc<Config>`：`segmenter()` 时 `Arc::clone` 即可，避免每条文本
    /// 深拷贝 `Config`（含 `DictPlan::Custom(Vec<DictSource>)`，评审 P1-11）。
    config: Arc<Config>,
}

impl TokenizerFactory {
    /// 根据配置创建一个新的工厂。
    /// 这个过程会加载所有词典，开销较大，建议在应用启动时只执行一次。
    /// 词典文件不存在时 panic。如需错误处理，请用 [`TokenizerFactory::try_new`]。
    pub fn new(config: Config) -> Self {
        Self {
            dictionary: Dictionary::new(&config),
            config: Arc::new(config),
        }
    }

    /// 尝试创建工厂，返回 Result 而非 panic
    pub fn try_new(config: Config) -> error::IkResult<Self> {
        let dictionary = Dictionary::try_new(&config)?;
        Ok(Self {
            dictionary,
            config: Arc::new(config),
        })
    }

    /// 创建一个用于处理特定文本的分词器迭代器。
    /// 这个操作非常轻量级。
    pub fn segmenter(&self, text: &str) -> IKSegmenter {
        IKSegmenter::new_from_factory(text, Arc::clone(&self.config), Arc::clone(&self.dictionary))
    }

    /// 创建零拷贝的词元**区间**迭代器（评审 P2-1）。
    ///
    /// 与 [`TokenizerFactory::segmenter`] 产出**完全相同的词元序列**（同一推进逻辑，
    /// 停用词过滤与数量词合并行为一致），但 [`Span`] 不含文本，因而**不做每词元
    /// 一次 `String` 分配**（该分配实测占分词热路径全部堆分配的 94%）。
    /// 适合检索、索引等需要自行拿原文切片的场景。
    ///
    /// ```
    /// use ik_analyzer::{Config, TokenizerFactory, char_to_byte_map};
    ///
    /// let factory = TokenizerFactory::new(Config::builder().build());
    /// let text = "欢迎使用本分词器";
    /// let map = char_to_byte_map(text); // 整篇构建一次，供全部 span 复用
    /// let words: Vec<&str> = factory
    ///     .segment_spans(text)
    ///     .map(|s| s.slice(text, &map))
    ///     .collect();
    /// assert_eq!(words, ["欢迎", "使用", "本", "分词器"]);
    /// ```
    pub fn segment_spans(&self, text: &str) -> SpanSegmenter {
        SpanSegmenter::new(self.segmenter(text))
    }

    /// 创建一个流式分词器，从 `impl Read` 读取数据。
    /// 适合处理大文件 / 网络流，恒定内存。
    pub fn segmenter_from_read<R: std::io::Read + Send + 'static>(&self, reader: R) -> IKSegmenter {
        IKSegmenter::new_from_read(
            reader,
            Arc::clone(&self.config),
            Arc::clone(&self.dictionary),
        )
    }

    /// 返回工厂内部使用的 Dictionary 实例的共享引用。
    ///
    /// 这对于需要直接访问词典进行底层操作（如基准测试）的场景很有用。
    pub fn dictionary(&self) -> Arc<Dictionary> {
        Arc::clone(&self.dictionary)
    }
}
