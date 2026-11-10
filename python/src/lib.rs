use pyo3::exceptions::PyValueError;
use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};

// --- 核心库的导入 ---
use ik_analyzer_core::AstralMode;
use ik_analyzer_core::Lexeme;
use ik_analyzer_core::TokenizerFactory;
use ik_analyzer_core::cfg::{Config, ConfigBuilder, DictSource};

/// 补充平面（astral，码点 > U+FFFF）字符的处理模式。
///
/// 对应核心库 `ik_analyzer.AstralMode`：
/// - `Drop`：全部非 BMP 字符丢弃（= IK 2012_u6 严格口径，
///   `Tokenizer.u6_parity()` 钉此值）；
/// - `Ideographs`（**默认**）：CJK 扩展 B–I 识别为汉字，其余补充平面
///   字符（emoji 等）丢弃；
/// - `Tokens`：全部非 BMP 字符输出为单字 `CN_CHAR` 词元（机制对齐
///   analysis-ik 的 `SurrogatePairSegmenter` 目标）。
#[pyclass(eq, module = "ik_analyzer", name = "AstralMode")]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PyAstralMode {
    Drop,
    Ideographs,
    Tokens,
}

impl From<PyAstralMode> for AstralMode {
    fn from(m: PyAstralMode) -> Self {
        match m {
            PyAstralMode::Drop => AstralMode::Drop,
            PyAstralMode::Ideographs => AstralMode::Ideographs,
            PyAstralMode::Tokens => AstralMode::Tokens,
        }
    }
}

/// A Lexeme object, containing detailed information of a segmentation result.
///
/// **注意**：`start` / `end` 是 **byte（UTF-8）偏移**，不是 Python 字符下标。
/// 这是为了便于直接用 Python 字符串的切片语义定位原文（`text[start:end]`）；
/// 但 Python 用户通常按 code point 理解下标，故务必注意区分。
/// 若需字符下标，请自行以 `text.encode()` 的长度换算，或改用 Rust 侧的
/// [`ik_analyzer_rs::segment_spans`]（返回全局 `char` 下标）。
#[pyclass(get_all, name = "Lexeme")]
#[derive(Debug, Clone)]
pub struct PyLexeme {
    /// 词元文本（**已归一化**：全角→半角，且默认 ASCII 小写化）
    pub text: String,
    /// 起始 **byte** 偏移（含）
    pub start: usize,
    /// 结束 **byte** 偏移（不含）
    pub end: usize,
    /// 词元类型名（如 `"CN_WORD"`、`"ARABIC"`）
    pub lexeme_type: String,
}

/// Configuration builder for the Tokenizer.
#[pyclass(name = "Config")]
#[derive(Clone, Default)]
pub struct PyConfigBuilder {
    builder: ConfigBuilder,
}

#[pymethods]
impl PyConfigBuilder {
    #[new]
    fn new() -> Self {
        Self {
            builder: Config::builder(),
        }
    }

    /// Sets the segmentation mode.
    ///
    /// Defaults to `True` (smart), matching v1.0.0 / IK 2012_u6.
    #[pyo3(signature = (use_smart))]
    fn use_smart(&mut self, use_smart: bool) {
        self.builder.set_use_smart(use_smart);
    }

    /// Enables/disables ASCII case folding of both input and dictionary words.
    ///
    /// Defaults to `True` (case-insensitive, matching v1.0.0 / IK 2012_u6).
    /// `False` keeps the original case on both sides (strict, case-sensitive).
    #[pyo3(signature = (enable))]
    fn enable_lowercase(&mut self, enable: bool) {
        self.builder.set_enable_lowercase(enable);
    }

    /// Sets the astral (non-BMP, e.g. emoji) character handling mode.
    ///
    /// Defaults to `AstralMode.Ideographs` (CJK Extension B–I recognized as
    /// Chinese, other astral characters dropped). See `AstralMode`.
    fn astral_mode(&mut self, mode: PyAstralMode) {
        self.builder.set_astral_mode(mode.into());
    }

    // --- `with_` / `add_ext_` 系列方法 ---
    //
    // 约定：一律调用 `ConfigBuilder` 的 `set_*`（`&mut self`）变体原地修改，
    // **不得**写成 `self.builder = self.builder.clone().with_x(..)` —— 那会深拷贝
    // 全部词源（含 `Cow::Owned` 的大文本扩展词典）。1.2.0 改了 9 处、漏了 3 处，
    // 收尾补齐后本文件不应再出现 `builder.clone().with_*` / `builder.clone().add_ext_*`
    // （评审 P4-7 / N-1）。`Tokenizer::new` 里的 `builder.clone().build()` 不在此列：
    // `build(self)` 按值消费，而 builder 由 pyclass 持有、无法移出，只能克隆。

    /// (Convenience) Replaces the main dictionary with a list of file paths.
    fn with_main_dict_from_paths(&mut self, paths: Vec<String>) {
        let sources = paths
            .into_iter()
            .map(|p| DictSource::File(p.into()))
            .collect();
        self.builder.set_with_main_dict(sources);
    }

    /// (Advanced) Replaces the main dictionary with a list of detailed sources.
    #[pyo3(signature = (sources))]
    fn with_main_dict_from_sources(&mut self, sources: &Bound<PyList>) -> PyResult<()> {
        let dict_sources = parse_dict_list_sources(sources)?;
        self.builder.set_with_main_dict(dict_sources);
        Ok(())
    }

    /// (Convenience) Replaces the quantifier dictionary with a list of file paths.
    fn with_quantifier_from_paths(&mut self, paths: Vec<String>) {
        let sources = paths
            .into_iter()
            .map(|p| DictSource::File(p.into()))
            .collect();
        self.builder.set_with_quantifier(sources);
    }

    /// (Advanced) Replaces the quantifier dictionary with a list of detailed sources.
    #[pyo3(signature = (sources))]
    fn with_quantifier_from_sources(&mut self, sources: &Bound<PyList>) -> PyResult<()> {
        let dict_sources = parse_dict_list_sources(sources)?;
        self.builder.set_with_quantifier(dict_sources);
        Ok(())
    }

    /// (Convenience) Replaces the stop word dictionaries with a list of file paths.
    fn with_stop_words_from_paths(&mut self, paths: Vec<String>) {
        let sources = paths
            .into_iter()
            .map(|p| DictSource::File(p.into()))
            .collect();
        self.builder.set_with_stop_words(sources);
    }

    /// (Advanced) Replaces the stop word dictionaries with a list of detailed sources.
    #[pyo3(signature = (sources))]
    fn with_stop_words_from_sources(&mut self, sources: &Bound<PyList>) -> PyResult<()> {
        let dict_sources = parse_dict_list_sources(sources)?;
        self.builder.set_with_stop_words(dict_sources);
        Ok(())
    }

    // --- `add_ext_` 系列方法 ---

    /// Appends extension dictionaries from file paths.
    fn add_ext_dict_from_paths(&mut self, paths: Vec<String>) {
        let paths: Vec<std::path::PathBuf> = paths.into_iter().map(Into::into).collect();
        self.builder.set_add_ext_dict_paths(paths);
    }

    /// Appends an extension dictionary from a multi-line text string.
    fn add_ext_dict_from_text(&mut self, text: &str) {
        self.builder.set_add_ext_dict_owned_text(text.to_string());
    }

    /// Appends extension stop word dictionaries from file paths.
    fn add_ext_stop_word_from_paths(&mut self, paths: Vec<String>) {
        let paths: Vec<std::path::PathBuf> = paths.into_iter().map(Into::into).collect();
        self.builder.set_add_ext_stop_word_paths(paths);
    }

    /// Appends an extension stop word dictionary from a multi-line text string.
    fn add_ext_stop_word_from_text(&mut self, text: &str) {
        self.builder
            .set_add_ext_stop_word_owned_text(text.to_string());
    }

    /// Appends extension quantifier dictionaries from file paths.
    fn add_ext_quantifier_from_paths(&mut self, paths: Vec<String>) {
        let paths: Vec<std::path::PathBuf> = paths.into_iter().map(Into::into).collect();
        self.builder.set_add_ext_quantifier_paths(paths);
    }

    /// Appends an extension quantifier dictionary from a multi-line text string.
    fn add_ext_quantifier_from_text(&mut self, text: &str) {
        self.builder
            .set_add_ext_quantifier_owned_text(text.to_string());
    }
}

/// The main Tokenizer class.
#[pyclass(name = "Tokenizer")]
struct Tokenizer {
    factory: TokenizerFactory,
}

#[pymethods]
impl Tokenizer {
    #[new]
    #[pyo3(signature = (config = None))]
    fn new(config: Option<PyRefMut<PyConfigBuilder>>) -> PyResult<Self> {
        let final_config = config.map_or_else(
            || Config::builder().build(),
            |cfg| cfg.builder.clone().build(),
        );
        // 用 `try_new` 替代 panic 版 `TokenizerFactory::new`：当用户通过
        // `with_*_from_paths` / `*_from_sources` 传入不存在的词典路径，或词典加载
        // 失败（IO 错误、非法 UTF-8 行）时，映射为 `PyValueError` 而非 Rust panic，
        // 避免 pyo3 扩展使 Python 解释器 abort（评审 P0-3）。
        let factory = TokenizerFactory::try_new(final_config)
            .map_err(|e| PyValueError::new_err(e.to_string()))?;
        Ok(Self { factory })
    }

    /// IK 2012_u6 behavior-parity Tokenizer (Rust: `Config::u6_parity`).
    ///
    /// Pins a frozen u6 configuration: u6 distribution dictionaries
    /// (`main2012.dic` + `ext.dic` + `quantifier.dic`), **no stop words**,
    /// ASCII case-insensitive, astral characters dropped. Behavior parity is
    /// guaranteed by this explicit preset rather than by default values —
    /// see the u6 parity preset above.
    ///
    /// ```python
    /// tok = ik.Tokenizer.u6_parity(use_smart=True)
    /// tok.segment("这是一个中文分词的例子")  # '的' not filtered
    /// ```
    #[staticmethod]
    #[pyo3(signature = (use_smart = true))]
    fn u6_parity(use_smart: bool) -> PyResult<Self> {
        let factory = TokenizerFactory::try_new(Config::u6_parity(use_smart))
            .map_err(|e| PyValueError::new_err(e.to_string()))?;
        Ok(Self { factory })
    }

    fn segment<'py>(&self, py: Python<'py>, text: &str) -> PyResult<Bound<'py, PyList>> {
        let results: Vec<String> = py.detach(|| {
            self.factory
                .segmenter(text)
                .map(|l| l.text().to_string())
                .collect()
        });
        PyList::new(py, results)
    }

    fn cut<'py>(&self, py: Python<'py>, text: &str) -> PyResult<Bound<'py, PyList>> {
        // char→byte 偏移映射（u32 减半内存，4GB 文本以内足够）
        let char_to_byte_map: Vec<u32> = text
            .char_indices()
            .map(|(byte_index, _)| byte_index as u32)
            .chain(std::iter::once(text.len() as u32))
            .collect();

        let results: Vec<PyLexeme> = py.detach(|| {
            self.factory
                .segmenter(text)
                .map(|l| {
                    let start_char_idx = l.begin_position() as usize;
                    let end_char_idx = l.end_position() as usize;

                    PyLexeme {
                        text: l.text().to_string(),
                        start: char_to_byte_map[start_char_idx] as usize,
                        end: char_to_byte_map[end_char_idx] as usize,
                        lexeme_type: lexeme_type_to_str(l.lexeme_type()).to_string(),
                    }
                })
                .collect()
        });
        PyList::new(py, results)
    }

    /// 批量分词：对多条文本释放 GIL 后**串行**处理（评审 P1-1）。
    ///
    /// 注：仅释放 GIL，未做多核并行；如需真并行可由调用方在 Python 侧用
    /// 线程池配合多个 `Tokenizer` 实例实现。命名沿用历史名称以兼容旧代码。
    fn segment_batch<'py>(
        &self,
        py: Python<'py>,
        texts: Vec<String>,
    ) -> PyResult<Bound<'py, PyList>> {
        let results: Vec<Vec<String>> = py.detach(|| {
            texts
                .into_iter()
                .map(|text| {
                    self.factory
                        .segmenter(&text)
                        .map(|l| l.text().to_string())
                        .collect()
                })
                .collect()
        });
        // 用 `collect` 一次性构造嵌套 PyList，替代 `map(...).unwrap()` 的写法，
        // 错误通过 `?` 传播（评审 P1-2）。
        let py_lists: Vec<Bound<'py, PyList>> = results
            .into_iter()
            .map(|v| PyList::new(py, v))
            .collect::<PyResult<_>>()?;
        PyList::new(py, py_lists)
    }
}

/// Helper function to convert i32 lexeme type to its string representation.
fn lexeme_type_to_str(t: i32) -> &'static str {
    match t {
        Lexeme::TYPE_ENGLISH => "ENGLISH",
        Lexeme::TYPE_ARABIC => "ARABIC",
        Lexeme::TYPE_LETTER => "LETTER",
        Lexeme::TYPE_CNWORD => "CN_WORD",
        Lexeme::TYPE_CNCHAR => "CN_CHAR",
        Lexeme::TYPE_OTHER_CJK => "OTHER_CJK",
        Lexeme::TYPE_COUNT => "COUNT",
        Lexeme::TYPE_CNUM => "CN_NUM",
        Lexeme::TYPE_CQUAN => "CN_QUAN",
        _ => "UNKNOWN",
    }
}

/// Parses a Python list of dictionaries into a `Vec<DictSource>`.
fn parse_dict_list_sources(sources: &Bound<PyList>) -> PyResult<Vec<DictSource>> {
    let mut dict_sources = Vec::new();
    for item in sources.iter() {
        let dict = item.downcast::<PyDict>()?;

        let has_path = dict.contains("path")?;
        let has_text = dict.contains("text")?;

        match (has_path, has_text) {
            (true, false) => {
                let path: String = dict.get_item("path")?.unwrap().extract()?;
                dict_sources.push(DictSource::File(path.into()));
            }
            (false, true) => {
                let text_string: String = dict.get_item("text")?.unwrap().extract()?;
                dict_sources.push(DictSource::text_owned(text_string));
            }
            (true, true) => {
                return Err(PyValueError::new_err(format!(
                    "Invalid source format: dictionary cannot contain both 'path' and 'text' keys: {:?}",
                    dict
                )));
            }
            (false, false) => {
                return Err(PyValueError::new_err(format!(
                    "Invalid source format: dictionary must contain either a 'path' or a 'text' key: {:?}",
                    dict
                )));
            }
        }
    }
    Ok(dict_sources)
}

/// The Python module definition.
#[pymodule]
fn ik_analyzer(m: &Bound<PyModule>) -> PyResult<()> {
    m.add_class::<Tokenizer>()?;
    m.add_class::<PyLexeme>()?;
    m.add_class::<PyConfigBuilder>()?;
    m.add_class::<PyAstralMode>()?;
    // 报告**核心库**版本而非 python crate 版本：两者曾漂移
    // （python 1.1.1 / 核心 1.1.2），使用手册还把旧值写成了示例输出，
    // 使 bug 固化为"预期行为"。版本现统一由 [workspace.package] 声明（评审 P3-1）。
    m.add("__version__", ik_analyzer_core::VERSION)?;
    // 另暴露 python crate 自身版本，便于诊断"绑定版本 vs 链接的核心库版本"
    m.add("__binding_version__", env!("CARGO_PKG_VERSION"))?;
    Ok(())
}
