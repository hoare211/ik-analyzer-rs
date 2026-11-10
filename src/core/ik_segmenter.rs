//! 分词器主入口（`IKSegmenter` / `SpanSegmenter` / `Segmenters`）。
//!
//! # 对齐基准
//!
//! 对应 analysis-ik `core/src/main/java/org/wltea/analyzer/core/IKSegmenter.java`
//! （`master@6d2d70f`）。与 u6（`IKSegmenter.java`）的差异：
//! ① 构造器形态：u6 有 `(Reader, boolean useSmart)` 便捷构造器且 `init()` 内
//!   自调 `Dictionary.initial(cfg)`；B 仅保留 `(Reader, Configuration)` 且词典
//!   初始化责任移交 ES 胶水层。本库以 `TokenizerFactory` 显式承担（与 B 的
//!   责任划分一致，无隐式单例初始化）；
//! ② **不注册 `SurrogatePairSegmenter`**：B 注册它输出 astral 词元，但实现有
//!   两个已实证缺陷（单字母 ENGLISH 紧邻丢失、跨页拆半个）；本库以
//!   `AstralMode`（见 `cfg`）在字符分类层实现同一目标且无其缺陷（决策 4）。

use crate::cfg::Config;
use crate::core::analyze_context::{AnalyzeContext, BUFF_SIZE};

use crate::core::char_source::{CharSource, ReadCharSource, StrCharSource};
use crate::core::cjk_segmenter::CJKSegmenter;
use crate::core::cn_quantifier_segmenter::CnQuantifierSegmenter;
use crate::core::ik_arbitrator::IKArbitrator;
use crate::core::isegmenter::ISegmenter;
use crate::core::letter_segmenter::LetterSegmenter;
use crate::core::lexeme::{Lexeme, Span};
use crate::dic::Dictionary;
use std::io::Read;
use std::sync::Arc;

/// 子分词器集合：使用具体类型而非 `Box<dyn ISegmenter>`，实现静态分发（monomorphization）。
/// 这样编译器能把每个 `analyze` 内联，消除 Java JIT 才能做的"去虚化"开销（原 `dyn` 每字符 3 次间接跳转）。
pub(crate) struct Segmenters {
    letter: LetterSegmenter,
    quan: CnQuantifierSegmenter,
    cjk: CJKSegmenter,
}

impl Segmenters {
    fn new() -> Self {
        Self {
            letter: LetterSegmenter::new(),
            quan: CnQuantifierSegmenter::new(),
            cjk: CJKSegmenter::new(),
        }
    }

    #[inline]
    fn analyze_all(&mut self, context: &mut AnalyzeContext) {
        self.letter.analyze(context);
        self.quan.analyze(context);
        self.cjk.analyze(context);
    }

    #[inline]
    fn reset_all(&mut self) {
        self.letter.reset();
        self.quan.reset();
        self.cjk.reset();
    }
}

/// IK 分词器。
///
/// 这个结构体实现了 `Iterator` trait，可以逐个地返回分词结果 (`Lexeme`)。
/// 对于处理大型文本或流式数据非常高效，因为它不会一次性将所有结果加载到内存中。
///
/// # 示例
/// ```
/// use ik_analyzer::{Config, TokenizerFactory};
///
/// let factory = TokenizerFactory::new(Config::builder().use_smart(true).build());
/// let mut segmenter = factory.segmenter("欢迎使用本分词器");
/// assert_eq!(segmenter.next().unwrap().text(), "欢迎");
/// assert_eq!(segmenter.next().unwrap().text(), "使用");
/// ```
pub struct IKSegmenter {
    char_source: Box<dyn CharSource>,
    cfg: Arc<Config>,
    context: AnalyzeContext,
    segmenters: Segmenters,
    arbitrator: IKArbitrator,
    /// 最近一次流式读取记录的 IO 错误（评审 P0-2）。
    ///
    /// `Iterator::next` 无法返回错误，故读取失败时只能"停止并返回已读到的部分"。
    /// 这里转存错误，使调用方能在迭代结束后经 [`IKSegmenter::take_error`] 精确判定，
    /// 而不必依赖日志后端（`env_logger` 已是 optional feature，下游默认无输出）。
    io_error: Option<std::io::Error>,
}

impl IKSegmenter {
    pub(crate) fn new_from_factory(
        input_str: &str,
        cfg: Arc<Config>,
        dictionary: Arc<Dictionary>,
    ) -> Self {
        let char_source: Box<dyn CharSource> = Box::new(StrCharSource::new(input_str));
        let buffer_size = char_source
            .len_hint()
            .map(|len| len.min(BUFF_SIZE))
            .unwrap_or(BUFF_SIZE);
        let context = AnalyzeContext::new(Arc::clone(&cfg), dictionary, buffer_size);
        let segmenters = Segmenters::new();
        let arbitrator = IKArbitrator::new();
        Self {
            char_source,
            cfg,
            context,
            segmenters,
            arbitrator,
            io_error: None,
        }
    }

    /// 从 `impl Read` 构造流式分词器（恒定内存，适合大文件/网络流）。
    /// 读取器需要 `Send` 以支持多线程。
    ///
    /// # 读取失败时的行为
    ///
    /// 读取出错时本库**无法**通过 `Iterator` 返回错误（`Item = Lexeme`），只能停止
    /// 并返回已读到的部分，即**内容被截断**。此时：
    ///
    /// - 会打一条 `log::error!`（需下游自行安装日志后端）；
    /// - 错误同时转存到本实例，可用 [`IKSegmenter::take_error`] / [`IKSegmenter::has_error`]
    ///   精确判定 —— 这是**推荐**的判定方式，不依赖日志。
    ///
    /// ```
    /// use ik_analyzer::{Config, TokenizerFactory};
    /// use std::io::Cursor;
    ///
    /// let factory = TokenizerFactory::new(Config::builder().build());
    /// let mut seg = factory.segmenter_from_read(Cursor::new("中文分词".as_bytes().to_vec()));
    /// let tokens: Vec<String> = seg.by_ref().map(|l| l.text().to_string()).collect();
    /// if seg.take_error().is_some() {
    ///     // 读取出错：tokens 内容可能不完整，需按业务决定是否接受
    /// }
    /// ```
    pub fn new_from_read<R: Read + Send + 'static>(
        reader: R,
        cfg: Arc<Config>,
        dictionary: Arc<Dictionary>,
    ) -> Self {
        let char_source: Box<dyn CharSource> = Box::new(ReadCharSource::new(reader));
        let context = AnalyzeContext::new(Arc::clone(&cfg), dictionary, BUFF_SIZE);
        let segmenters = Segmenters::new();
        let arbitrator = IKArbitrator::new();
        Self {
            char_source,
            cfg,
            context,
            segmenters,
            arbitrator,
            io_error: None,
        }
    }

    /// 取走并返回流式读取期间记录的 IO 错误（若有）。
    ///
    /// 迭代结束后调用一次即可判定本次分词是否因读取失败而**内容不完整**
    /// （评审 P0-2）。内存路径（[`crate::TokenizerFactory::segmenter`]）恒返回 `None`。
    pub fn take_error(&mut self) -> Option<std::io::Error> {
        self.io_error.take()
    }

    /// 流式读取期间是否发生过 IO 错误（不取走，可重复查询）。
    pub fn has_error(&self) -> bool {
        self.io_error.is_some()
    }

    // new 方法接收 &str，并在内部创建数据的副本
    #[deprecated(
        since = "1.0.0",
        note = "每次调用都会重建词典，开销大。请使用 TokenizerFactory::segmenter 共享词典"
    )]
    pub fn new(input_str: &str, use_smart: bool) -> Self {
        let config = Config::builder().use_smart(use_smart).build();
        Self::new_with_cfg(input_str, config)
    }

    pub fn new_with_cfg(input_str: &str, cfg: Config) -> Self {
        let cfg = Arc::new(cfg);
        let dictionary = Dictionary::new(&cfg);
        let char_source: Box<dyn CharSource> = Box::new(StrCharSource::new(input_str));
        let buffer_size = char_source
            .len_hint()
            .map(|len| len.min(BUFF_SIZE))
            .unwrap_or(BUFF_SIZE);
        let context = AnalyzeContext::new(Arc::clone(&cfg), Arc::clone(&dictionary), buffer_size);

        let segmenters = Segmenters::new();
        let arbitrator = IKArbitrator::new();

        Self {
            char_source,
            cfg,
            context,
            segmenters,
            arbitrator,
            io_error: None,
        }
    }

    fn process_buffer(&mut self) {
        if self.context.available > 0 {
            self.context.init_cursor();
            loop {
                self.segmenters.analyze_all(&mut self.context);
                if self.context.need_refill_buffer() {
                    break;
                }
                if !self.context.move_cursor() {
                    break;
                }
            }

            self.segmenters.reset_all();
            self.arbitrator
                .process(&mut self.context, self.cfg.use_smart);
            self.context.output_to_result();
            self.context.mark_buffer_offset();
        }
    }
}

// Iterator 的 impl 块不再需要生命周期
impl Iterator for IKSegmenter {
    type Item = Lexeme;

    fn next(&mut self) -> Option<Self::Item> {
        self.advance(true)
    }
}

impl IKSegmenter {
    /// 分词推进一格：取出下一个已定稿词元（已合并/已滤停用词）。
    ///
    /// `with_text` 决定是否为该词元填充文本（见
    /// [`AnalyzeContext::get_next_lexeme`]）。两条输出路径共用本函数，
    /// 保证词元序列与停用词/数量词处理完全一致。
    fn advance(&mut self, with_text: bool) -> Option<Lexeme> {
        loop {
            if let Some(lexeme) = self.context.get_next_lexeme(with_text) {
                return Some(lexeme);
            }

            if self.context.is_finished() && self.context.results_is_empty() {
                // 流式读取期间可能记录了 IO 错误（不再被静默吞掉）。
                // 迭代结束时透出日志，并转存到 `self.io_error` 供调用方经
                // `take_error()` 精确判定 —— 日志只是 best-effort 提示，下游默认
                // 没有日志后端（`env_logger` 为 optional feature），故不能只靠它（评审 P0-2）。
                if let Some(io_err) = self.char_source.take_io_error() {
                    log::error!(
                        "[ik-analyzer-rs] stream read error (content may be truncated): {}",
                        io_err
                    );
                    // 保留首个错误（take 语义已清空 char_source 侧记录）
                    if self.io_error.is_none() {
                        self.io_error = Some(io_err);
                    }
                }
                return None;
            }

            // 调用 fill_buffer，从 CharSource 填充数据
            let filled_new = self.context.fill_buffer(self.char_source.as_mut());

            if self.context.available > 0 {
                self.process_buffer();
            } else if !filled_new {
                self.context.finish();
            }
        }
    }
}

/// 零拷贝的词元区间迭代器（评审 P2-1）。
///
/// 由 [`crate::TokenizerFactory::segment_spans`] 构造。产出的 [`Span`] 只含位置与类型，
/// **不做每词元一次的 `String` 文本分配** —— 该分配实测占分词热路径全部堆分配的
/// 94%（`examples/alloc_count.rs`）。调用方用自己的原文切片还原词元：
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
///
/// 词元序列与 [`crate::TokenizerFactory::segmenter`] 完全一致（同一推进逻辑），
/// 停用词过滤、数量词合并行为相同。
pub struct SpanSegmenter {
    inner: IKSegmenter,
}

impl SpanSegmenter {
    /// 由 `IKSegmenter` 构造（仅限 crate 内部与工厂方法使用）。
    pub(crate) fn new(inner: IKSegmenter) -> Self {
        Self { inner }
    }

    /// 取走并返回流式读取期间记录的 IO 错误（若有）。
    ///
    /// 与 [`IKSegmenter::take_error`] 语义相同；本迭代器只支持内存输入，
    /// 故实际恒为 `None`，保留此方法是为了与 [`IKSegmenter`] 的 API 形状一致，
    /// 便于泛型代码统一处理。
    pub fn take_error(&mut self) -> Option<std::io::Error> {
        self.inner.take_error()
    }
}

impl Iterator for SpanSegmenter {
    type Item = Span;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.advance(false).map(|l| Span::from_lexeme(&l))
    }
}
