use crate::cfg::{Config, DictPlan, DictSource, default_dict_sources, u6_dict_sources};
use crate::dic::compact_trie::CompactTrie;
use crate::dic::dawg_builder::DawgBuilder;
use crate::dic::hit::Hit;
use crate::dic::persistent_hit::PersistentHit;
use crate::error::{IkError, IkResult};
use log::info;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::sync::{Arc, Mutex, OnceLock};

/// 词典管理类
///
/// 词典管理类，负责词典的加载、构建以及提供词典查询接口。
/// 其实例在构建后是不可变的。
///
/// # 对齐基准
///
/// 对应 analysis-ik `dic/src/main/java/org/wltea/analyzer/dic/Dictionary.java`
/// （`master@6d2d70f`）。与 u6（`Dictionary.java`，362 行）的差异：
/// ① 单例 → `Arc`（多词典/多配置进程内共存，u6 `getSingleton()` 做不到）；
/// ② **词典侧大小写**：u6 6 处载入路径全部 `trim().toLowerCase()`（全 Unicode）；
///    analysis-ik 本地词典不折叠（`Dictionary.java:207`）——那是**功能退化**
///    （含大写词条永久失配，A 级实测调研报告 §5.3）。本库采构建期 ASCII 折叠
///    （与 `enable_lowercase` 联动），= u6 语义；
/// ③ **BOM 剥离**（决策 5，B:199-200 有、u6 无）；
/// ④ 远程热词 + `Monitor` 60s 轮询、ext 目录递归、`reLoadMainDict`、
///    surname/suffix/preposition 三部「加载即丢弃」词典——**均不吸收**
///    （与库定位冲突 / B 侧缺陷，见设计决策 6/7）；
/// ⑤ u6 `isStopWord` 的整段成词语义两侧一致，本库保持（评审 P1-1 护栏）。
///
/// 每部词典是一个运行期从文本词表构建的 DAWG（`CompactTrie`，方案 C'）。
/// 纯默认词表命中全局缓存，进程内只建一次。
pub struct Dictionary {
    main: Arc<CompactTrie>,
    stop: Arc<CompactTrie>,
    quant: Arc<CompactTrie>,
}

// 为 DictLoadInfo 添加 Clone trait
#[derive(Clone)]
pub(crate) struct DictLoadInfo {
    dict_type: String,
    source: String,
    word_count: usize,
}

/// 默认词表类别（全局缓存键的一部分）。
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum DictKind {
    Main,
    Stop,
    Quant,
}

/// 词表集标识（全局缓存键的一部分）：区分「默认词表」与「u6 parity 钉死词表」。
///
/// 两者今天内容相同，但**语义不同**：`Default` 允许随版本扩容（设计方案
/// 决策 7），`U6` 是 parity 契约的一部分、永不改变。分立缓存键保证扩容后
/// 两套词表互不污染。
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum DictSet {
    Default,
    U6,
}

impl DictSet {
    fn name(self) -> &'static str {
        match self {
            DictSet::Default => "default",
            DictSet::U6 => "u6-parity",
        }
    }
}

/// 全局 DAWG 缓存的值类型（评审 P2-3：抽出类型别名降低复杂度）。
type DawgCache = HashMap<(DictSet, DictKind, bool), Arc<CompactTrie>>;

/// 纯默认词表 DAWG 的全局缓存：键 = (词表集, 词典, enable_lowercase)，至多 12 项，懒构建。
///
/// 仅 `DictPlan::Default` / `DictPlan::U6` 命中缓存；`Custom`（含 `add_ext_*`）
/// 全量重建、不缓存——`File` 源内容可在两次构建之间变化，按源身份缓存有
/// 正确性风险（方案 §3.7）。
/// 并发首次构建可能重复劳动（双跑后写），结果确定性一致，benign race。
static DEFAULT_DAWG_CACHE: OnceLock<Mutex<DawgCache>> = OnceLock::new();

fn default_dawg_cache() -> &'static Mutex<DawgCache> {
    DEFAULT_DAWG_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn cache_get(set: DictSet, kind: DictKind, lowercase: bool) -> Option<Arc<CompactTrie>> {
    default_dawg_cache()
        .lock()
        .unwrap()
        .get(&(set, kind, lowercase))
        .cloned()
}

fn cache_put(set: DictSet, kind: DictKind, lowercase: bool, ctrie: Arc<CompactTrie>) {
    default_dawg_cache()
        .lock()
        .unwrap()
        .insert((set, kind, lowercase), ctrie);
}

/// 词典侧的大小写策略（方案 C' §4：构建期归一化，替代旧方案的匹配期折叠）。
/// `lowercase=true` 时 ASCII 大写折叠为小写（与原版 `+32` 语义一致），
/// 同词大小写变体在构建期自然合并；`false` 保留原文（严格 A 语义）。
fn to_word_chars(word: &str, lowercase: bool) -> Vec<char> {
    word.chars()
        .map(|c| {
            if lowercase && c.is_ascii_uppercase() {
                c.to_ascii_lowercase()
            } else {
                c
            }
        })
        .collect()
}

/// 词表行清洗：去首尾空白，并剥离可能的前导 UTF-8 BOM（U+FEFF）。
///
/// # 空白裁剪集口径（u6 对齐 + 一处有意改进）
///
/// 裁剪字符集 = **码点 ≤ U+0020**（u6 `String.trim()` 的精确语义：控制字符 +
/// ASCII 空白）**外加 U+00A0（NBSP）**：
///
/// - **≤ U+0020**：与 u6 逐字一致（`Dictionary.java` 六处 `trim()`）；
/// - **U+00A0**：**有意改进**（issue 20261012 §7.4）——community.dic 尾附 NBSP
///   的词条（如 `方正凤凰城\u{a0}\u{a0}\u{a0}\u{a0}`，:169476）u6 因不裁 NBSP
///   而不可命中，本库裁后可命中，行为更优故保留；
/// - **不含 U+3000（CJK 全角空格）等其他 Unicode 空白**：u6 同样不裁。裁它会
///   **静默改变词条集合**——region.dic:249714 的 `\u{3000}村` 一旦被裁成 `村`，
///   `村` 即进入仲裁候选集，`深潭村` 被切为 `[深潭, 村]` 而非 u6 的 `[深, 潭村]`
///   （同 issue 根因：22 行"XY村"村名的切分点系统性偏移）。
///
/// BOM：u6 不剥离（`InputStreamReader("UTF-8")`，带 BOM 的词典首词条被污染）；
/// analysis-ik 显式剥离（`Dictionary.java:199-200`）。本仓库自带词典均无 BOM，
/// 此处的受益者是用户词典与未来换用 analysis-ik 词典的场景——**纯防御**，
/// 对格式正确的词表零行为变更（设计决策 5）。
fn clean_dict_word(line: &str) -> &str {
    let trimmed = trim_dict_ws(line);
    let stripped = trimmed.trim_start_matches('\u{FEFF}');
    trim_dict_ws(stripped)
}

/// 词典行首尾空白裁剪：u6 `String.trim()` 语义（码点 ≤ U+0020）+ NBSP（有意改进）。
///
/// **不用** `str::trim()`（Unicode `White_Space` 全集）：那会连带裁掉 U+3000 等
/// u6 不裁的字符，静默扩充词条集合（见 `clean_dict_word` 文档与 issue 20261012）。
#[inline]
fn trim_dict_ws(line: &str) -> &str {
    line.trim_matches(|c: char| c <= '\u{0020}' || c == '\u{00A0}')
}

impl Dictionary {
    pub fn new(cfg: &Config) -> Arc<Self> {
        Self::try_new(cfg).expect("Failed to build Dictionary")
    }

    /// 尝试构建词典，返回 Result 而非 panic
    pub fn try_new(cfg: &Config) -> IkResult<Arc<Self>> {
        info!("[ik-analyzer-rs] Building new dictionaries instance...");

        let lowercase = cfg.enable_lowercase();
        let mut load_infos = Vec::new();
        let main = Self::build_dict(
            &cfg.main,
            DictKind::Main,
            "Main",
            lowercase,
            &mut load_infos,
        )?;
        let stop = Self::build_dict(
            &cfg.stop,
            DictKind::Stop,
            "StopWord",
            lowercase,
            &mut load_infos,
        )?;
        let quant = Self::build_dict(
            &cfg.quant,
            DictKind::Quant,
            "Quantifier",
            lowercase,
            &mut load_infos,
        )?;

        let report = Self::format_load_report(&load_infos);
        if !report.is_empty() {
            info!("{}", report);
        }

        info!("[ik-analyzer-rs] Dictionaries instance built successfully.");
        Ok(Arc::new(Dictionary { main, stop, quant }))
    }

    /// 根据构建计划构建一部词典（统一 DAWG 路径，方案 C' §3.4）。
    fn build_dict(
        plan: &DictPlan,
        kind: DictKind,
        dict_type: &str,
        lowercase: bool,
        log: &mut Vec<DictLoadInfo>,
    ) -> IkResult<Arc<CompactTrie>> {
        match plan {
            DictPlan::Default => {
                Self::build_cached(DictSet::Default, kind, dict_type, lowercase, log)
            }
            DictPlan::U6 => Self::build_cached(DictSet::U6, kind, dict_type, lowercase, log),
            DictPlan::Custom(sources) => Self::build_dawg(sources, lowercase, dict_type, log),
        }
    }

    /// 内嵌词表集（`Default` / `U6`）的构建路径：命中全局缓存则直接复用
    /// （进程内只建一次），否则从对应词表源构建并写入缓存。
    fn build_cached(
        set: DictSet,
        kind: DictKind,
        dict_type: &str,
        lowercase: bool,
        log: &mut Vec<DictLoadInfo>,
    ) -> IkResult<Arc<CompactTrie>> {
        if let Some(ctrie) = cache_get(set, kind, lowercase) {
            log.push(DictLoadInfo {
                dict_type: dict_type.to_string(),
                source: format!("{} embedded text (cached DAWG)", set.name()),
                word_count: ctrie.count_words(),
            });
            return Ok(ctrie);
        }
        let (main_src, stop_src, quant_src) = match set {
            DictSet::Default => default_dict_sources(),
            DictSet::U6 => u6_dict_sources(),
        };
        let sources = match kind {
            DictKind::Main => main_src,
            DictKind::Stop => stop_src,
            DictKind::Quant => quant_src,
        };
        let ctrie = Self::build_dawg(&sources, lowercase, dict_type, log)?;
        cache_put(set, kind, lowercase, Arc::clone(&ctrie));
        Ok(ctrie)
    }

    /// 从词源构建 DAWG：逐词经 `to_word_chars`（构建期大小写策略）后插入
    /// `DawgBuilder`，finish 得到字节，`CompactTrie` 零拷贝读回。
    fn build_dawg(
        sources: &[DictSource],
        lowercase: bool,
        dict_type: &str,
        log: &mut Vec<DictLoadInfo>,
    ) -> IkResult<Arc<CompactTrie>> {
        let mut builder = DawgBuilder::new();
        for source in sources {
            match source {
                DictSource::File(path) => {
                    // 不做 `path.exists()` 预检（TOCTOU 且冗余）：直接打开，失败时把路径塞进
                    // 错误消息，避免 `From<io::Error>` 丢失路径信息。
                    let file = File::open(path).map_err(|e| {
                        IkError::Io(std::io::Error::new(
                            e.kind(),
                            format!("failed to open dictionary '{}': {}", path.display(), e),
                        ))
                    })?;
                    let reader = BufReader::new(file);
                    for (line_no, line) in reader.lines().enumerate() {
                        match line {
                            Ok(word) => {
                                let word = clean_dict_word(&word);
                                if !word.is_empty() {
                                    builder.insert(&to_word_chars(word, lowercase));
                                }
                            }
                            Err(e) => {
                                // 非法 UTF-8 行不应静默丢弃：词典少词极难排查，
                                // 直接返回 `InvalidEncoding` 让上层感知（评审 P0-2）。
                                return Err(IkError::InvalidEncoding {
                                    path: path.display().to_string(),
                                    line: line_no + 1,
                                    source: Box::new(e),
                                });
                            }
                        }
                    }
                }
                DictSource::Text(text) => {
                    for line in text.lines() {
                        let word = clean_dict_word(line);
                        if !word.is_empty() {
                            builder.insert(&to_word_chars(word, lowercase));
                        }
                    }
                }
            }
        }
        let word_count = builder.word_count();
        let bytes: Arc<[u8]> = Arc::from(builder.finish());
        let ctrie = Arc::new(CompactTrie::from_bytes(bytes));
        log.push(DictLoadInfo {
            dict_type: dict_type.to_string(),
            source: "text sources (runtime-built DAWG)".to_string(),
            word_count: word_count as usize,
        });
        Ok(ctrie)
    }

    /// 格式化加载信息为简单文本
    fn format_load_report(infos: &[DictLoadInfo]) -> String {
        if infos.is_empty() {
            return String::new();
        }

        let mut report = String::from("\n[ik-analyzer-rs] Dictionary Loading Report:\n");
        for info in infos {
            report.push_str(&format!(
                "  {:<10} | {:<50} | {:>6} words | Loaded\n",
                info.dict_type, info.source, info.word_count
            ));
        }
        report
    }

    pub fn match_in_main_dict(&self, char_array: &[char], begin: usize, length: usize) -> Hit {
        let end = (begin + length).min(char_array.len());
        self.main.match_text(&char_array[begin..end], begin as i32)
    }

    pub fn match_in_quantifier_dict(
        &self,
        char_array: &[char],
        begin: usize,
        length: usize,
    ) -> Hit {
        let end = (begin + length).min(char_array.len());
        self.quant.match_text(&char_array[begin..end], begin as i32)
    }

    pub fn match_with_hit(
        &self,
        char_array: &[char],
        current_index: usize,
        p_hit: PersistentHit,
    ) -> Hit {
        let mut search_hit = Hit::new();
        search_hit.set_begin(p_hit.begin());
        self.main.continue_match(
            p_hit.node_idx(),
            char_array[current_index],
            current_index as i32,
            &mut search_hit,
        );
        search_hit
    }

    pub fn match_with_hit_in_quantifier(
        &self,
        char_array: &[char],
        current_index: usize,
        p_hit: PersistentHit,
    ) -> Hit {
        let mut search_hit = Hit::new();
        search_hit.set_begin(p_hit.begin());
        self.quant.continue_match(
            p_hit.node_idx(),
            char_array[current_index],
            current_index as i32,
            &mut search_hit,
        );
        search_hit
    }

    /// 判断 `char_array[begin .. begin+length]` 是否**整段**命中停用词。
    ///
    /// 采用**整段成词**语义（而非"路径上任一位置成词即命中"），这与
    /// [`crate::Dictionary::match_in_main_dict`] 一致，且是停用词过滤正确的必要条件：
    /// 调用点（`analyze_context::get_next_lexeme`）传入的是**词元长度**（`length > 1`），
    /// 若用前缀语义，任何"前缀命中停用词"的长词元都会被误删
    /// （推导见 `compact_trie::match_text` 的模块内文档）。
    pub fn is_stop_word(&self, char_array: &[char], begin: usize, length: usize) -> bool {
        let end = (begin + length).min(char_array.len());
        self.stop
            .match_text(&char_array[begin..end], begin as i32)
            .is_match()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cfg::{Config, DictSource};
    use std::fs::File;
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn test_custom_dictionary_loading_from_file() {
        // 1. 在临时目录中创建测试用的词典文件
        let dir = TempDir::new().unwrap();
        let custom_dict_path = dir.path().join("my_dict.dic");
        let mut file = File::create(&custom_dict_path).unwrap();
        writeln!(file, "自定义词").unwrap();
        writeln!(file, "测试").unwrap();

        // 2. 使用 ConfigBuilder 构建一个只使用我们自定义词典的配置
        //    通过 with_main_dict 覆盖默认词典，确保测试环境纯净。
        let config = Config::builder()
            .with_main_dict(vec![DictSource::File(custom_dict_path)])
            .build();

        // 3. 直接调用 Dictionary::new() 创建一个全新的、独立的实例
        //    不再有任何全局状态或单例的烦恼。
        let dict = Dictionary::new(&config);

        // 4. 执行断言，验证词典是否按预期加载
        let chars_match: Vec<char> = "自定义词".chars().collect();
        let hit_match = dict.match_in_main_dict(&chars_match, 0, chars_match.len());
        assert!(hit_match.is_match(), "应该匹配到 '自定义词'");
        assert!(!hit_match.is_prefix());

        let chars_also_match: Vec<char> = "测试".chars().collect();
        let hit_also_match = dict.match_in_main_dict(&chars_also_match, 0, chars_also_match.len());
        assert!(hit_also_match.is_match(), "应该匹配到 '测试'");

        let chars_no_match: Vec<char> = "不存在".chars().collect();
        let hit_no_match = dict.match_in_main_dict(&chars_no_match, 0, chars_no_match.len());
        assert!(hit_no_match.is_unmatch(), "不应该匹配到 '不存在'");

        // 验证内置词典确实已被禁用
        let chars_default_word: Vec<char> = "中国".chars().collect();
        let hit_default = dict.match_in_main_dict(&chars_default_word, 0, chars_default_word.len());
        assert!(
            hit_default.is_unmatch(),
            "内置词典 '中国' 应该已被禁用，不应匹配到"
        );
    }

    #[test]
    fn test_stopwords_and_quantifiers_loading_from_text() {
        // 1. 准备内存中的词典文本
        let stop_words_text = "的\n了\n吗";
        let quantifier_text = "个\n只\n匹";

        // 2. 使用 ConfigBuilder 构建一个完全自定义的配置
        let config = Config::builder()
            .with_main_dict(vec![])
            .with_stop_words(vec![DictSource::text(stop_words_text)])
            .with_quantifier(vec![DictSource::text(quantifier_text)])
            .build();

        // 3. 创建一个全新的、独立的 Dictionary 实例
        let dict = Dictionary::new(&config);

        // 4. 断言：测试停用词
        let chars_stop: Vec<char> = "的".chars().collect();
        assert!(
            dict.is_stop_word(&chars_stop, 0, 1),
            "'的' 应该是一个停用词"
        );

        let chars_not_stop: Vec<char> = "我".chars().collect();
        assert!(
            !dict.is_stop_word(&chars_not_stop, 0, 1),
            "'我' 不应该是一个停用词"
        );

        // 5. 断言：测试量词
        let text_buffer: Vec<char> = "三个苹果".chars().collect();

        // 测试 '个' 是否在量词词典中
        let hit_quantifier_match = dict.match_in_quantifier_dict(&text_buffer, 1, 1);
        assert!(
            hit_quantifier_match.is_match(),
            "'个' 应该在量词词典中匹配到"
        );

        // 测试 '苹' 是否在量词词典中
        let hit_quantifier_no_match = dict.match_in_quantifier_dict(&text_buffer, 2, 1);
        assert!(
            hit_quantifier_no_match.is_unmatch(),
            "'苹' 不应该在量词词典中匹配到"
        );
    }

    #[test]
    fn test_default_dictionary_loading() {
        // 1. 使用默认配置
        let config = Config::builder().build();

        // 2. 创建一个使用默认配置的 Dictionary 实例
        let dict = Dictionary::new(&config);

        // 3. 验证默认主词典中的词可以被匹配
        let chars_china: Vec<char> = "中国".chars().collect();
        let hit_china = dict.match_in_main_dict(&chars_china, 0, chars_china.len());
        assert!(hit_china.is_match(), "默认主词典中的 '中国' 应该能被匹配");

        // 4. 验证默认停用词词典中的词是停用词
        let chars_stop: Vec<char> = "然而".chars().collect(); // "然而" 在默认停用词典中
        assert!(
            dict.is_stop_word(&chars_stop, 0, chars_stop.len()),
            "默认停用词 '然而' 应该被识别"
        );
    }

    /// 默认词表缓存：同配置两次构建共享同一份 DAWG（Arc::ptr_eq）。
    #[test]
    fn test_default_dict_cache_sharing() {
        let cfg = Config::builder().build();
        let dict1 = Dictionary::new(&cfg);
        let dict2 = Dictionary::new(&cfg);
        // 借查询行为确认两实例可用，再以内部缓存判定共享
        let probe: Vec<char> = "中国".chars().collect();
        assert!(dict1.match_in_main_dict(&probe, 0, 2).is_match());
        assert!(dict2.match_in_main_dict(&probe, 0, 2).is_match());
        let cached1 = cache_get(DictSet::Default, DictKind::Main, true).expect("缓存应已填充");
        let cached2 = cache_get(DictSet::Default, DictKind::Main, true).expect("缓存应已填充");
        assert!(
            Arc::ptr_eq(&cached1, &cached2),
            "纯默认词表应命中同一份缓存 DAWG"
        );
    }

    /// Default 与等价 Custom（同一文本源）查询结果完全一致（单后端等价性，方案 §5.1）。
    #[test]
    fn test_default_plan_equals_custom_plan_with_same_sources() {
        let default_dict = Dictionary::new(&Config::builder().build());

        // 用与默认主词典完全相同的文本源构建 Custom 计划
        let cfg = Config::builder()
            .with_main_dict(vec![
                DictSource::text(include_str!("../../dict/main2012.dic")),
                DictSource::text(include_str!("../../dict/ext_dict/ext.dic")),
            ])
            .build();
        let custom_dict = Dictionary::new(&cfg);

        for word in ["中国", "中华人民共和国", "分词", "不存在于词典的词组"] {
            let chars: Vec<char> = word.chars().collect();
            let hit_default = default_dict.match_in_main_dict(&chars, 0, chars.len());
            let hit_custom = custom_dict.match_in_main_dict(&chars, 0, chars.len());
            assert_eq!(
                hit_default.is_match(),
                hit_custom.is_match(),
                "词 '{word}' 在 Default/Custom 两路径的命中性应一致"
            );
            assert_eq!(
                hit_default.is_prefix(),
                hit_custom.is_prefix(),
                "词 '{word}' 在 Default/Custom 两路径的前缀语义应一致"
            );
        }
    }

    /// 严格 A 语义（构建期机制，方案 C' §4）：false 词典保留原文严格匹配；
    /// true 词典构建期小写化合并，单路径完备（旧折叠方案的分叉遮蔽结构性消失）。
    #[test]
    fn test_strict_case_semantics_build_time() {
        fn hits(dict: &Dictionary, word: &str) -> bool {
            let chars: Vec<char> = word.chars().collect();
            dict.match_in_main_dict(&chars, 0, chars.len()).is_match()
        }

        // false（严格 A 语义）：词典保留原文，raw 精确匹配，`a` ≠ `A`
        let cfg = Config::builder()
            .enable_lowercase(false)
            .with_main_dict(vec![DictSource::text("A股\napp")])
            .build();
        let dict = Dictionary::new(&cfg);
        assert!(hits(&dict, "A股"), "严格模式应命中词典原文词 'A股'");
        assert!(
            !hits(&dict, "a股"),
            "严格模式小写查询不应命中 'A股'（A 语义）"
        );
        assert!(hits(&dict, "app"));

        // true（默认）：词典构建期小写化；大小写变体分叉词两侧均可达、无遮蔽
        let cfg2 = Config::builder()
            .enable_lowercase(true)
            .with_main_dict(vec![DictSource::text("A股\napp")])
            .build();
        let dict2 = Dictionary::new(&cfg2);
        assert!(
            hits(&dict2, "a股"),
            "true 模式词典已归一为 'a股'，小写查询应命中"
        );
        assert!(
            !hits(&dict2, "A股"),
            "匹配核心为精确比较，大写查询不命中已归一词典（需输入侧 regularize）"
        );
        // 旧折叠方案的 single-path 贪心遮蔽（词典含 A股/app 时查 app 不可达）在此结构性不存在
        assert!(hits(&dict2, "app"), "true 模式下 'app' 不应被 'a' 分叉遮蔽");
    }

    /// 大小写变体词在 true 模式构建期合并（去重）：AB/ab/Ab 归一为同一词。
    #[test]
    fn test_case_variant_merge_in_lowercase_mode() {
        fn hits(dict: &Dictionary, word: &str) -> bool {
            let chars: Vec<char> = word.chars().collect();
            dict.match_in_main_dict(&chars, 0, chars.len()).is_match()
        }
        let cfg = Config::builder()
            .enable_lowercase(true)
            .with_main_dict(vec![DictSource::text("AB\nab\nAb")])
            .build();
        let dict = Dictionary::new(&cfg);
        assert!(hits(&dict, "ab"), "变体词应合并为 'ab' 并可命中");
        assert!(!hits(&dict, "AB"), "精确匹配：已归一词典不应被大写查询命中");
        assert!(!hits(&dict, "aB"), "混合大小写查询不应命中");
    }

    /// 决策 5：词表前导 UTF-8 BOM（U+FEFF）被剥离，首词条不被污染。
    /// u6 不剥离（首词条被污染），analysis-ik 剥离（`Dictionary.java:199-200`）。
    #[test]
    fn test_dict_bom_is_stripped() {
        fn hits(dict: &Dictionary, word: &str) -> bool {
            let chars: Vec<char> = word.chars().collect();
            dict.match_in_main_dict(&chars, 0, chars.len()).is_match()
        }
        // 文本源带 BOM
        let cfg = Config::builder()
            .with_main_dict(vec![DictSource::text("\u{FEFF}中国\n美国")])
            .build();
        let dict = Dictionary::new(&cfg);
        assert!(hits(&dict, "中国"), "BOM 不应污染首词条 '中国'");
        assert!(hits(&dict, "美国"), "后续词条不受影响");
        // 文件源带 BOM（Windows 编辑的 .dic 常见形态）
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("bom.dic");
        let mut file = File::create(&path).unwrap();
        file.write_all("\u{FEFF}中国\n美国\n".as_bytes()).unwrap();
        drop(file);
        let cfg_file = Config::builder()
            .with_main_dict(vec![DictSource::File(path)])
            .build();
        let dict_file = Dictionary::new(&cfg_file);
        assert!(hits(&dict_file, "中国"), "文件源 BOM 也应被剥离");
        assert!(hits(&dict_file, "美国"), "文件源后续词条不受影响");
    }

    /// issue 20261012：词典行空白裁剪集 = u6 `String.trim()` 语义（码点 ≤ U+0020）
    /// **外加 U+00A0（NBSP，有意改进）**；**不含 U+3000** 等其他 Unicode 空白
    /// （u6 同样不裁；裁了会静默改变词条集合——region.dic:249714 的 `\u{3000}村`
    /// 被裁成 `村` 后，`深潭村` 类切分与 u6 不一致）。
    #[test]
    fn test_dict_word_cleaning_u6_trim_set_plus_nbsp() {
        fn hits(dict: &Dictionary, word: &str) -> (bool, bool) {
            let chars: Vec<char> = word.chars().collect();
            let h = dict.match_in_main_dict(&chars, 0, chars.len());
            (h.is_match(), h.is_prefix())
        }

        // ASCII 空白与控制字符：与 u6 trim() 逐字一致地被裁掉
        let cfg = Config::builder()
            .with_main_dict(vec![DictSource::text(" 中国 \n\t美国\x01\n")])
            .build();
        let dict = Dictionary::new(&cfg);
        assert_eq!(hits(&dict, "中国"), (true, false));
        assert_eq!(hits(&dict, "美国"), (true, false));

        // NBSP（U+00A0）：有意裁剪——方正凤凰城类尾附 NBSP 词条保持可命中（issue §7.4）
        let cfg = Config::builder()
            .with_main_dict(vec![DictSource::text(
                "方正凤凰城\u{00A0}\u{00A0}\u{00A0}\u{00A0}",
            )])
            .build();
        let dict = Dictionary::new(&cfg);
        assert_eq!(hits(&dict, "方正凤凰城"), (true, false));

        // U+3000（CJK 全角空格）：**不裁**（u6 语义）——污染行保持为 `\u{3000}村`，
        // 独立的 `村` 词条不会凭空出现（深潭村 issue 根因）
        let cfg = Config::builder()
            .with_main_dict(vec![DictSource::text("深潭\n潭村\n\u{3000}村")])
            .build();
        let dict = Dictionary::new(&cfg);
        assert_eq!(hits(&dict, "深潭"), (true, false));
        assert_eq!(hits(&dict, "潭村"), (true, false));
        assert_eq!(
            hits(&dict, "村"),
            (false, false),
            "U+3000 前缀词条不得被裁成独立词 '村'（u6 同样不裁 U+3000）"
        );
    }

    /// 决策 1：`Config::u6_parity` 的三部词典计划 = `U6`（钉死词表集），
    /// 停用词为空表；且 U6 计划命中独立缓存（两次构建共享 DAWG）。
    #[test]
    fn test_u6_parity_plan_and_cache() {
        // 停用词置空：内置 102 词一个都不是停用词
        let parity = Dictionary::new(&Config::u6_parity(true));
        let probe: Vec<char> = "的".chars().collect();
        assert!(
            !parity.is_stop_word(&probe, 0, 1),
            "u6_parity 的停用词表应为空（'的' 不再是停用词）"
        );
        // 主词典仍可用（u6 分发词表）
        let china: Vec<char> = "中国".chars().collect();
        assert!(
            parity.match_in_main_dict(&china, 0, 2).is_match(),
            "u6_parity 主词典应可用"
        );
        // U6 计划命中独立缓存键：两次 u6_parity 构建共享同一份 DAWG
        let cached1 = cache_get(DictSet::U6, DictKind::Main, true).expect("U6 缓存应已填充");
        let cached2 = cache_get(DictSet::U6, DictKind::Main, true).expect("U6 缓存应已填充");
        assert!(
            Arc::ptr_eq(&cached1, &cached2),
            "u6_parity 应命中同一份 U6 缓存 DAWG"
        );
        // 与 Default 集分立：默认配置的主词典是另一份缓存条目
        let _default = Dictionary::new(&Config::builder().build());
        let cached_default =
            cache_get(DictSet::Default, DictKind::Main, true).expect("默认缓存应已填充");
        assert!(
            !Arc::ptr_eq(&cached1, &cached_default),
            "U6 与 Default 必须是分立缓存键（未来默认扩容不得污染 parity 词表）"
        );
    }
}
