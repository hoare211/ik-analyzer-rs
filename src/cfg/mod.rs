use std::borrow::Cow;
use std::path::PathBuf;

/// 补充平面（astral，码点 > U+FFFF）字符的处理模式。
///
/// IK 2012_u6 与 analysis-ik 均以 Java `char`（UTF-16 码元）为工作单元：
/// 补充平面字符被拆为代理对，落入 `CHAR_USELESS` 丢弃（analysis-ik 另以
/// `SurrogatePairSegmenter` 输出，但该实现有两个已实证缺陷——单字母 ENGLISH
/// 紧邻代理对时丢失、跨页代理对被拆成半个，见
/// 设计决策 4）。本 crate 以 Rust `char`
/// （Unicode 标量值）直接处理，行为由本枚举显式选择。
///
/// # 变体
/// - [`AstralMode::Drop`]：IK 2012_u6 严格口径；
/// - [`AstralMode::Ideographs`]：**默认**，CJK 扩展 B–I 识别为汉字
///   （u6 的丢弃是 Java `char` 的技术局限而非设计，且 analysis-ik 同样输出）；
/// - [`AstralMode::Tokens`]：机制对齐 analysis-ik 的 `SurrogatePairSegmenter`
///   目标（全部非 BMP 可成为 token），但不照搬其缺陷。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AstralMode {
    /// 全部非 BMP 字符丢弃（= IK 2012_u6 严格口径）。
    /// [`Config::u6_parity`] 钉此值。
    Drop,
    /// **默认**：CJK 统一表意文字扩展 B–I 识别为汉字（可参与词典匹配），
    /// 其余非 BMP 字符（emoji、符号、变体标签等）丢弃。
    #[default]
    Ideographs,
    /// 全部非 BMP 字符输出为单字 `CNCHAR` 词元（可同时参与词典匹配）。
    Tokens,
}

/// 内嵌默认词表（**文本，唯一真源**，方案 C'，见 DAWG 构建方案）。
/// main = main2012 + ext_dict（与原 build.rs 词源一致）。运行期由 `Dictionary` 经
/// `DawgBuilder` 现建 DAWG；纯默认词表命中全局缓存，进程内只建一次。
/// git 中不再存在任何二进制词典工件（旧 `dict_compiled/` 已删除）。
pub(crate) fn default_dict_sources() -> (Vec<DictSource>, Vec<DictSource>, Vec<DictSource>) {
    (
        vec![
            DictSource::text(include_str!("../../dict/main2012.dic")),
            DictSource::text(include_str!("../../dict/ext_dict/ext.dic")),
        ],
        vec![DictSource::text(include_str!(
            "../../dict/ext_stop_word/stop.dic"
        ))],
        vec![DictSource::text(include_str!("../../dict/quantifier.dic"))],
    )
}

/// u6 parity 预设的钉死词表（**永不修改**：内容 = 移植时的 u6 分发词表）。
///
/// 与 [`default_dict_sources`] 分离是 parity 契约的结构性保证：未来默认词表
/// 扩容（「u6 + 精选增量」，设计决策 7）只改 `default_dict_sources()`，
/// **不得改本函数**，否则 [`Config::u6_parity`] 会被静默破坏。
pub(crate) fn u6_dict_sources() -> (Vec<DictSource>, Vec<DictSource>, Vec<DictSource>) {
    (
        vec![
            DictSource::text(include_str!("../../dict/main2012.dic")),
            DictSource::text(include_str!("../../dict/ext_dict/ext.dic")),
        ],
        vec![], // u6 bare core 口径：无停用词（parity 契约的配置映射表）
        vec![DictSource::text(include_str!("../../dict/quantifier.dic"))],
    )
}

// --- Part 1: 配置定义 ---

/// 词源：文件或内存文本。
#[derive(Debug, Clone)]
pub enum DictSource {
    File(PathBuf),
    Text(Cow<'static, str>),
}

impl DictSource {
    pub fn text(s: &'static str) -> Self {
        DictSource::Text(Cow::Borrowed(s))
    }

    pub fn text_owned(s: String) -> Self {
        DictSource::Text(Cow::Owned(s))
    }
}

/// 一部词典的「构建计划」（方案 C'）：
/// - `Default`：纯默认词表（内嵌文本）。构建时命中全局缓存，进程内只建一次 DAWG。
/// - `U6`：u6 parity 预设的钉死词表（[`u6_dict_sources`]）。命中**独立**缓存键，
///   不随 `Default` 词表的未来扩容而变化。
/// - `Custom`：完全指定的词表（`with_*` 覆盖，或默认词表 + `add_ext_*` 物化后的源
///   列表）。每次全量构建 DAWG、不缓存——`File` 源内容可在两次构建之间变化，
///   按源身份缓存有正确性风险（方案 §3.7）。
#[derive(Debug, Clone)]
pub(crate) enum DictPlan {
    Default,
    U6,
    Custom(Vec<DictSource>),
}

/// IK 分词器的配置。
///
/// 此配置是不可变的。要更改配置，请创建一个新的 `Config` 实例。
#[derive(Debug, Clone)]
pub struct Config {
    pub(crate) use_smart: bool,
    pub(crate) enable_lowercase: bool,
    pub(crate) astral: AstralMode,
    pub(crate) main: DictPlan,
    pub(crate) stop: DictPlan,
    pub(crate) quant: DictPlan,
}

impl Config {
    /// 创建配置构建器
    pub fn builder() -> ConfigBuilder {
        ConfigBuilder::new()
    }

    /// IK 2012_u6 行为 parity 预设（设计决策 1）。
    ///
    /// 钉住一份**完整且冻结**的 u6 口径配置：
    /// - 词典 = u6 分发词表（`main2012.dic` + `ext.dic` + `quantifier.dic`），
    ///   经内部函数 `u6_dict_sources()` 直接引用，不随默认词表的未来扩容而漂移；
    /// - 停用词 = 空（u6 bare core 口径，见 parity 契约的配置映射表）；
    /// - `enable_lowercase = true`（= u6 无条件 ASCII 小写）；
    /// - [`AstralMode::Drop`]（= u6 对补充平面字符的丢弃口径）。
    ///
    /// 与「改默认值」相比，parity 由此**显式契约**保证：依赖库一行
    /// `Config::u6_parity(smart)` 即得可差分验证的 u6 行为，不依赖默认值
    /// 巧合。预设冻结——需要「u6 + 自定义词」时请用
    /// [`ConfigBuilder::with_main_dict`] 显式装配，并自行承担 parity 责任。
    pub fn u6_parity(use_smart: bool) -> Config {
        Config {
            use_smart,
            enable_lowercase: true,
            astral: AstralMode::Drop,
            main: DictPlan::U6,
            stop: DictPlan::U6,
            quant: DictPlan::U6,
        }
    }

    /// 是否对输入及词典词做 ASCII 小写化（大小写不敏感）。
    /// 默认 `true`：与原版 IK 默认行为一致（输出小写、大小写不敏感）。
    /// 设为 `false`：保留原文大小写，词典按原文（含大小写）严格匹配
    /// （`a股` ≠ `A股`，对齐 analysis-ik 5.0 的 `enableLowercase=false`）。
    pub fn enable_lowercase(&self) -> bool {
        self.enable_lowercase
    }

    /// 补充平面（astral）字符处理模式，默认 [`AstralMode::Ideographs`]。
    pub fn astral_mode(&self) -> AstralMode {
        self.astral
    }
}

/// 配置构建器，用于方便地创建不可变的 `Config`。
///
/// # 方法语义
/// - `with_main_dict` / `with_stop_words` / `with_quantifier`：
///   **替换**对应默认词典（包括内置默认）。传空 `vec![]` 即清空该部词典。
/// - `add_ext_dict_*` / `add_ext_stop_word_*` / `add_ext_quantifier_*`：
///   **追加**到对应默认词典之后（默认词典仍保留），常用于运行时热扩展词库。
#[derive(Clone)]
pub struct ConfigBuilder {
    use_smart: bool,
    enable_lowercase: bool,
    astral: AstralMode,
    main_dictionary: Option<Vec<DictSource>>,
    stop_words: Option<Vec<DictSource>>,
    quantifier: Option<Vec<DictSource>>,
    ext_dictionary: Vec<DictSource>,
    ext_stop_words: Vec<DictSource>,
    ext_quantifier: Vec<DictSource>,
}

impl ConfigBuilder {
    /// 默认配置：**与 v1.0.0（= IK Analyzer 2012_u6）行为一致**。
    ///
    /// | 配置项 | 默认值 | 语义 |
    /// |---|---|---|
    /// | `use_smart` | `true` | smart（歧义消解）模式，v1.0.0 `ConfigBuilder::new()` 的默认值 |
    /// | `enable_lowercase` | `true` | 输入与词典均做 ASCII 大小写归一（大小写不敏感） |
    /// | `astral_mode` | [`AstralMode::Ideographs`] | CJK 扩展 B–I 识别为汉字，其余补充平面字符（emoji 等）丢弃 |
    /// | 词典 | 内置 | `main2012.dic` + `ext.dic` + `quantifier.dic` + `stop.dic` |
    ///
    /// 另有两项「与输入形态无关」的归一（两版一致，不可关闭）：
    /// 输入侧全角→半角（`regularize`），故全角/半角写法产出相同词元；
    /// 词典侧不做全角归一（词典词须写半角，与原版一致）。
    ///
    /// **与 u6 的行为 parity 不由默认值保证**，而由 [`Config::u6_parity`]
    /// 显式契约保证（差异仅在停用词默认装载与 astral 口径，均可用配置达到
    /// u6 行为）。详见 [`Config::u6_parity`]。
    pub fn new() -> Self {
        ConfigBuilder {
            use_smart: true,
            enable_lowercase: true,
            astral: AstralMode::Ideographs,
            main_dictionary: None,
            stop_words: None,
            quantifier: None,
            ext_dictionary: Vec::new(),
            ext_stop_words: Vec::new(),
            ext_quantifier: Vec::new(),
        }
    }

    /// 设置智能分词模式。
    pub fn use_smart(mut self, use_smart: bool) -> Self {
        self.use_smart = use_smart;
        self
    }

    /// `use_smart` 的 `&mut self` 变体，供动态语言绑定（如 pyo3）原地修改，
    /// 避免 `self.builder.clone().use_smart(v)` 的 clone 噪音（评审 P3-1）。
    pub fn set_use_smart(&mut self, use_smart: bool) {
        self.use_smart = use_smart;
    }

    /// 是否对输入及词典词做 ASCII 小写化（大小写不敏感，默认 `true`）。
    /// 吸收自 analysis-ik 5.0 的 `enableLowercase` 开关（机制为构建期策略，方案 C'）：
    /// 词典侧在 DAWG 构建期经 `to_word_chars` 决定小写化与否，
    /// 变体词在 `true` 模式下自然合并；`false` 为严格 A 语义（`a股` ≠ `A股`）。
    pub fn enable_lowercase(mut self, enable: bool) -> Self {
        self.enable_lowercase = enable;
        self
    }

    /// `enable_lowercase` 的 `&mut self` 变体（见 `set_use_smart`）。
    pub fn set_enable_lowercase(&mut self, enable: bool) {
        self.enable_lowercase = enable;
    }

    /// 设置补充平面（astral）字符处理模式，默认 [`AstralMode::Ideographs`]。
    ///
    /// 吸收自 analysis-ik 5.0 的 `SurrogatePairSegmenter` 机制目标（变体
    /// [`AstralMode::Tokens`]），但**不照搬其两个已实证缺陷**（单字母
    /// ENGLISH 紧邻代理对时丢失、跨页代理对被拆成半个），详见
    /// 设计决策 4。
    pub fn astral_mode(mut self, mode: AstralMode) -> Self {
        self.astral = mode;
        self
    }

    /// `astral_mode` 的 `&mut self` 变体（见 `set_use_smart`）。
    pub fn set_astral_mode(&mut self, mode: AstralMode) {
        self.astral = mode;
    }

    /// 替换主词典（覆盖内置默认）。传空向量即清空主词典。
    pub fn with_main_dict(mut self, main_dictionary: Vec<DictSource>) -> Self {
        self.set_with_main_dict(main_dictionary);
        self
    }

    /// `with_main_dict` 的 `&mut self` 变体，供动态语言绑定（如 pyo3）原地修改，
    /// 避免 `self.builder.clone().with_main_dict(..)` 深拷贝全部词源
    /// （含 `Cow::Owned` 大文本扩展词典，评审 P4-7）。
    pub fn set_with_main_dict(&mut self, main_dictionary: Vec<DictSource>) {
        self.main_dictionary = Some(main_dictionary);
    }

    /// 替换量词词典（覆盖内置默认）。传空向量即清空量词词典。
    pub fn with_quantifier(mut self, quantifier: Vec<DictSource>) -> Self {
        self.set_with_quantifier(quantifier);
        self
    }

    /// `with_quantifier` 的 `&mut self` 变体，供动态语言绑定（如 pyo3）原地修改，
    /// 避免 `self.builder.clone().with_quantifier(..)` 深拷贝全部词源
    /// （含 `Cow::Owned` 大文本扩展词典，评审 P4-7）。
    pub fn set_with_quantifier(&mut self, quantifier: Vec<DictSource>) {
        self.quantifier = Some(quantifier);
    }

    /// 替换停用词词典（覆盖内置默认）。传空向量即清空停用词词典。
    pub fn with_stop_words(mut self, stop_words: Vec<DictSource>) -> Self {
        self.set_with_stop_words(stop_words);
        self
    }

    /// `with_stop_words` 的 `&mut self` 变体，供动态语言绑定（如 pyo3）原地修改，
    /// 避免 `self.builder.clone().with_stop_words(..)` 深拷贝全部词源
    /// （含 `Cow::Owned` 大文本扩展词典，评审 P4-7）。
    pub fn set_with_stop_words(&mut self, stop_words: Vec<DictSource>) {
        self.stop_words = Some(stop_words);
    }

    /// 追加主词典扩展词源（保留内置默认）。
    pub fn add_ext_dict_paths(mut self, paths: Vec<PathBuf>) -> Self {
        self.set_add_ext_dict_paths(paths);
        self
    }

    /// `add_ext_dict_paths` 的 `&mut self` 变体，供动态语言绑定（如 pyo3）原地修改，
    /// 避免 `self.builder.clone().add_ext_dict_paths(..)` 深拷贝全部词源
    /// （含 `Cow::Owned` 大文本扩展词典，评审 P4-7）。
    pub fn set_add_ext_dict_paths(&mut self, paths: Vec<PathBuf>) {
        self.ext_dictionary
            .extend(paths.into_iter().map(DictSource::File));
    }

    /// 追加主词典扩展词源（内存文本）。
    pub fn add_ext_dict_text(mut self, text: &'static str) -> Self {
        self.set_add_ext_dict_text(text);
        self
    }

    /// `add_ext_dict_text` 的 `&mut self` 变体，供动态语言绑定（如 pyo3）原地修改，
    /// 避免 `self.builder.clone().add_ext_dict_text(..)` 深拷贝全部词源
    /// （含 `Cow::Owned` 大文本扩展词典，评审 P4-7）。
    pub fn set_add_ext_dict_text(&mut self, text: &'static str) {
        self.ext_dictionary.push(DictSource::text(text));
    }

    /// 追加主词典扩展词源（拥有所有权的字符串）。
    pub fn add_ext_dict_owned_text(mut self, text: String) -> Self {
        self.set_add_ext_dict_owned_text(text);
        self
    }

    /// `add_ext_dict_owned_text` 的 `&mut self` 变体，供动态语言绑定（如 pyo3）原地修改，
    /// 避免 `self.builder.clone().add_ext_dict_owned_text(..)` 深拷贝全部词源
    /// （含 `Cow::Owned` 大文本扩展词典，评审 P4-7）。
    pub fn set_add_ext_dict_owned_text(&mut self, text: String) {
        self.ext_dictionary.push(DictSource::text_owned(text));
    }

    /// 追加停用词扩展词源（内存文本）。
    pub fn add_ext_stop_word_text(mut self, text: &'static str) -> Self {
        self.set_add_ext_stop_word_text(text);
        self
    }

    /// `add_ext_stop_word_text` 的 `&mut self` 变体，供动态语言绑定（如 pyo3）原地修改，
    /// 避免 `self.builder.clone().add_ext_stop_word_text(..)` 深拷贝全部词源
    /// （含 `Cow::Owned` 大文本扩展词典，评审 P4-7）。
    pub fn set_add_ext_stop_word_text(&mut self, text: &'static str) {
        self.ext_stop_words.push(DictSource::text(text));
    }

    /// 追加停用词扩展词源（拥有所有权的字符串）。
    pub fn add_ext_stop_word_owned_text(mut self, text: String) -> Self {
        self.set_add_ext_stop_word_owned_text(text);
        self
    }

    /// `add_ext_stop_word_owned_text` 的 `&mut self` 变体，供动态语言绑定（如 pyo3）原地修改，
    /// 避免 `self.builder.clone().add_ext_stop_word_owned_text(..)` 深拷贝全部词源
    /// （含 `Cow::Owned` 大文本扩展词典，评审 P4-7）。
    pub fn set_add_ext_stop_word_owned_text(&mut self, text: String) {
        self.ext_stop_words.push(DictSource::text_owned(text));
    }

    /// 追加停用词扩展词源（文件路径）。
    pub fn add_ext_stop_word_paths(mut self, paths: Vec<PathBuf>) -> Self {
        self.set_add_ext_stop_word_paths(paths);
        self
    }

    /// `add_ext_stop_word_paths` 的 `&mut self` 变体，供动态语言绑定（如 pyo3）原地修改，
    /// 避免 `self.builder.clone().add_ext_stop_word_paths(..)` 深拷贝全部词源
    /// （含 `Cow::Owned` 大文本扩展词典，评审 P4-7）。
    pub fn set_add_ext_stop_word_paths(&mut self, paths: Vec<PathBuf>) {
        self.ext_stop_words
            .extend(paths.into_iter().map(DictSource::File));
    }

    /// 追加量词扩展词源（内存文本）。
    pub fn add_ext_quantifier_text(mut self, text: &'static str) -> Self {
        self.set_add_ext_quantifier_text(text);
        self
    }

    /// `add_ext_quantifier_text` 的 `&mut self` 变体，供动态语言绑定（如 pyo3）原地修改，
    /// 避免 `self.builder.clone().add_ext_quantifier_text(..)` 深拷贝全部词源
    /// （含 `Cow::Owned` 大文本扩展词典，评审 P4-7）。
    pub fn set_add_ext_quantifier_text(&mut self, text: &'static str) {
        self.ext_quantifier.push(DictSource::text(text));
    }

    /// 追加量词扩展词源（拥有所有权的字符串）。
    pub fn add_ext_quantifier_owned_text(mut self, text: String) -> Self {
        self.set_add_ext_quantifier_owned_text(text);
        self
    }

    /// `add_ext_quantifier_owned_text` 的 `&mut self` 变体，供动态语言绑定（如 pyo3）原地修改，
    /// 避免 `self.builder.clone().add_ext_quantifier_owned_text(..)` 深拷贝全部词源
    /// （含 `Cow::Owned` 大文本扩展词典，评审 P4-7）。
    pub fn set_add_ext_quantifier_owned_text(&mut self, text: String) {
        self.ext_quantifier.push(DictSource::text_owned(text));
    }

    /// 追加量词扩展词源（文件路径）。
    pub fn add_ext_quantifier_paths(mut self, paths: Vec<PathBuf>) -> Self {
        self.set_add_ext_quantifier_paths(paths);
        self
    }

    /// `add_ext_quantifier_paths` 的 `&mut self` 变体，供动态语言绑定（如 pyo3）原地修改，
    /// 避免 `self.builder.clone().add_ext_quantifier_paths(..)` 深拷贝全部词源
    /// （含 `Cow::Owned` 大文本扩展词典，评审 P4-7）。
    pub fn set_add_ext_quantifier_paths(&mut self, paths: Vec<PathBuf>) {
        self.ext_quantifier
            .extend(paths.into_iter().map(DictSource::File));
    }

    /// 构建最终的 `Config`。
    ///
    /// 默认值（主/停用/量词）来自内嵌文本词表（运行期现建 DAWG，纯默认命中全局缓存）；
    /// 仅当用户显式覆盖（`with_*`）或追加（`add_ext_*`）时，才走不缓存的定制构建路径。
    pub fn build(self) -> Config {
        let (main_src, stop_src, quant_src) = default_dict_sources();
        let main = build_one(main_src, self.main_dictionary, self.ext_dictionary);
        let stop = build_one(stop_src, self.stop_words, self.ext_stop_words);
        let quant = build_one(quant_src, self.quantifier, self.ext_quantifier);
        Config {
            use_smart: self.use_smart,
            enable_lowercase: self.enable_lowercase,
            astral: self.astral,
            main,
            stop,
            quant,
        }
    }
}

impl Default for ConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// 根据「是否覆盖 + 是否追加扩展」决定一部词典的构建计划。
fn build_one(
    default_sources: Vec<DictSource>,
    override_opt: Option<Vec<DictSource>>,
    ext: Vec<DictSource>,
) -> DictPlan {
    // 快速路径：既无覆盖也无扩展 → 纯默认词表（全局缓存，进程内只建一次 DAWG）
    if override_opt.is_none() && ext.is_empty() {
        return DictPlan::Default;
    }
    // 覆盖：用户源 + 扩展；追加：物化默认文本源 + 扩展（该路径不复用缓存，全量重建）
    let mut sources = override_opt.unwrap_or(default_sources);
    sources.extend(ext);
    DictPlan::Custom(sources)
}
