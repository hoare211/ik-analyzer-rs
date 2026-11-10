// 确保在项目根目录下有一个 tests/ 目录
// 并在 Cargo.toml 中指明这是一个集成测试 (如果需要的话，但通常cargo会自动发现)

#![allow(deprecated)]

use ik_analyzer::{AstralMode, Config, DictSource, TokenizerFactory, char_to_byte_map};
use std::collections::HashSet;
use std::sync::Arc;

/// 共享的工厂实例：词典（27.6 万词）只构建一次，所有用例复用。
/// 避免每个 `IKSegmenter::new` 重复重建 Trie（debug 下尤为缓慢）。
/// `use_smart` 不同的两种模式各持有一个工厂。
fn shared_factories() -> &'static (Arc<TokenizerFactory>, Arc<TokenizerFactory>) {
    use std::sync::OnceLock;
    static FACTORIES: OnceLock<(Arc<TokenizerFactory>, Arc<TokenizerFactory>)> = OnceLock::new();
    FACTORIES.get_or_init(|| {
        (
            Arc::new(TokenizerFactory::new(
                Config::builder().use_smart(true).build(),
            )),
            Arc::new(TokenizerFactory::new(
                Config::builder().use_smart(false).build(),
            )),
        )
    })
}

#[allow(deprecated)]
fn segment_to_string(text: &str, use_smart: bool) -> String {
    // 复用共享工厂（只构建一次词典），生产代码同样推荐此用法
    let factories = shared_factories();
    let factory = if use_smart {
        &factories.0
    } else {
        &factories.1
    };
    let segmenter = factory.segmenter(text);

    let results: Vec<String> = segmenter.map(|lexeme| lexeme.text().to_string()).collect();
    results.join("/")
}

// --- 测试用例 1.1 ---
#[test]
fn test_basic_chinese_sentence_smart() {
    let input = "这是一个中文分词的例子";
    // 注：智能模式下停用词"的"会被过滤
    let expected = "这是/一个/中文/分词/例子";
    let result = segment_to_string(input, true);
    assert_eq!(
        result, expected,
        "Smart mode mismatch for input: '{}'",
        input
    );
}

#[test]
fn test_basic_chinese_sentence_fine_grained() {
    let input = "这是一个中文分词的例子";
    // 注：智能/细粒度模式下停用词"的"会被过滤
    let expected = "这是/一个/一/个中/个/中文/分词/例子";
    let result = segment_to_string(input, false);
    assert_eq!(
        result, expected,
        "Fine-grained mode mismatch for input: '{}'",
        input
    );
}

// --- 测试用例 1.2 ---
#[test]
fn test_mixed_content_sentence_smart() {
    let input = "IKAnalyzer2012是一个轻量级的中文分词工具包";
    // 注：智能模式下停用词"的"会被过滤；enable_lowercase 默认 true，
    // 字母 token 输出小写（恢复与 2012_u6 原版一致的默认行为）
    let expected = "ikanalyzer2012/是/一个/轻量级/中文/分词/工具包";
    let result = segment_to_string(input, true);
    assert_eq!(
        result, expected,
        "Smart mode mismatch for input: '{}'",
        input
    );
}

#[test]
fn test_mixed_content_sentence_fine_grained() {
    let input = "IKAnalyzer2012是一个轻量级的中文分词工具包";
    // 注：停用词"的"会被过滤；字母 token 小写化（同上）
    let expected =
        "ikanalyzer2012/ikanalyzer/2012/是/一个/一/个/轻量级/量级/中文/分词/工具包/工具/包";
    let result = segment_to_string(input, false);
    assert_eq!(
        result, expected,
        "Fine-grained mode mismatch for input: '{}'",
        input
    );
}

// --- enable_lowercase 开关特性测试（吸收自 analysis-ik 5.0，机制为构建期策略，方案 C'）---
#[test]
fn test_enable_lowercase_true_lowercases_letter_tokens() {
    let input = "HelloWorld 中文分词";
    let factory = TokenizerFactory::new(
        Config::builder()
            .use_smart(true)
            .enable_lowercase(true)
            .build(),
    );
    let tokens: Vec<String> = factory
        .segmenter(input)
        .map(|l| l.text().to_string())
        .collect();
    assert!(
        tokens.contains(&"helloworld".to_string()),
        "enable_lowercase=true 时字母 token 应小写化，实际: {tokens:?}"
    );
}

#[test]
fn test_enable_lowercase_false_preserves_letter_case() {
    let input = "HelloWorld 中文分词";
    let factory = TokenizerFactory::new(
        Config::builder()
            .use_smart(true)
            .enable_lowercase(false)
            .build(),
    );
    let tokens: Vec<String> = factory
        .segmenter(input)
        .map(|l| l.text().to_string())
        .collect();
    assert!(
        tokens.contains(&"HelloWorld".to_string()),
        "enable_lowercase=false 时应保留原文大小写，实际: {tokens:?}"
    );
    assert!(
        !tokens.contains(&"helloworld".to_string()),
        "enable_lowercase=false 时不应出现小写化 token"
    );
}

// 辅助函数，将分词结果字符串转换为一个无序的集合
fn to_token_set(s: &str) -> HashSet<&str> {
    s.split('/').collect()
}

/// 收集 `(begin_position, end_position, text)` 三元组，便于做位置层面断言。
/// 用**全局**坐标（跨页后仍连续），这正是 P0-1 被污染的那两个量。
fn collect_spans(factory: &TokenizerFactory, text: &str) -> Vec<(usize, usize, String)> {
    factory
        .segmenter(text)
        .map(|l| {
            (
                l.begin_position() as usize,
                l.end_position() as usize,
                l.text().to_string(),
            )
        })
        .collect()
}

/// 位置单调、不重叠、不越界（**smart 模式专用**）。
///
/// fine 模式按设计会输出交叉词元（如 中华人民 / 中华 / 华人 / 人民 位置重叠），
/// 故不适用本断言；fine 模式只要求 [`assert_tail_covered`]。
fn assert_positions_monotonic(spans: &[(usize, usize, String)], ctx: &str) {
    let mut prev_end = 0usize;
    for (b, e, text) in spans {
        assert!(
            b < e,
            "{ctx}: 词元 {text:?} 的 begin({b}) >= end({e})，长度非法"
        );
        assert!(
            *b >= prev_end,
            "{ctx}: 位置重叠或回退：prev_end={prev_end}, 词元 {b}-{e} {text:?}（坐标系混用？）"
        );
        prev_end = *e;
    }
}

/// 输出的最大结束位置必须等于输入字符数 —— 抓「尾部字符被静默丢弃」。
fn assert_tail_covered(spans: &[(usize, usize, String)], n_chars: usize, ctx: &str) {
    let max_end = spans.iter().map(|(_, e, _)| *e).max().unwrap_or(0);
    assert_eq!(
        max_end,
        n_chars,
        "{ctx}: 输出仅覆盖到 {max_end}，输入共 {n_chars} 字，尾部 {} 字被丢弃",
        n_chars - max_end
    );
}

/// 逐位置全覆盖（允许一个位置被多个词元覆盖，fine 模式会交叉）。
fn assert_full_coverage(spans: &[(usize, usize, String)], n_chars: usize, ctx: &str) {
    let mut covered = vec![false; n_chars];
    for (b, e, text) in spans {
        assert!(
            *e <= n_chars,
            "{ctx}: 词元 {b}-{e} {text:?} 越界（输入 {n_chars} 字）"
        );
        for slot in covered.iter_mut().take(*e).skip(*b) {
            *slot = true;
        }
    }
    if let Some(missing) = covered.iter().position(|c| !*c) {
        panic!(
            "{ctx}: 位置 {missing} 未被任何词元覆盖；未覆盖段 {:?}",
            {
                // 打印第一段连续未覆盖区间，便于定位
                let start = missing;
                let end = covered[start..]
                    .iter()
                    .position(|c| *c)
                    .map_or(n_chars, |i| start + i);
                (start, end)
            }
        );
    }
}

/// 换页测试语料：`占`×padding + `中华人民共和国` + `万岁`。
///
/// 刻意选用**不含任何停用词**的字符（已实测：`占`/`中华人民共和国`/`万岁` 均不在
/// `stop.dic`），从而"输出逐位置全覆盖"成为一条合法可断言的不变量。
fn paging_corpus(padding: usize) -> String {
    let mut s = String::with_capacity(padding + 16);
    s.extend(std::iter::repeat_n('占', padding));
    s.push_str("中华人民共和国");
    s.push_str("万岁");
    s
}

#[test]
fn test_no_duplicate_output_across_buffer_boundary() {
    // 构造 4200 字输入（必须 > 4096，会触发换页），统计输出字符总数是否等于输入。
    // 修复前：每换一次页多输出 1 个字符（cursor 处字符被重复）。
    let padding_char = '占';
    let n = 4200;
    let input: String = std::iter::repeat_n(padding_char, n).collect();

    let segmenter = shared_factories().0.segmenter(&input);
    let output_chars: usize = segmenter.map(|l| l.text().chars().count()).sum();

    assert_eq!(
        output_chars, n,
        "输出字符总数 ({}) != 输入字符总数 ({})，换页逻辑有重复输出或丢失",
        output_chars, n
    );
}

/// 【回归·评审 P0-1】换页不得丢失任何字符，且词元位置必须连续覆盖输入。
///
/// 缺陷背景：`analyze_context::output_to_result` 曾把**全局**坐标
/// `Lexeme::end_position()`（= `offset + begin + length`）当作**页内**下标赋给
/// `index`。第 1 页 `buff_offset == 0` 时两坐标系恰好相等，故完全隐匿；第 2 页起
/// `index` 一步跳出页内的 `cursor`，`while index <= self.cursor` 立即退出，
/// 该页剩余字符被**静默丢弃**（实测恒丢末尾 2 字，阈值 padding ≥ 4092）。
///
/// 为什么旧断言抓不到：旧断言是 `sum(text.chars().count()) == n_chars`，而丢失的
/// 字符会**同时从分子和分母消失**，总数仍然相等。故此处断言位置层面的不变量。
#[test]
fn test_paging_full_position_coverage() {
    // padding 扫描必须覆盖换页阈值：实测阈值在 4092，此前测试固定 4090 恰好在阈值
    // 之下，这是 P0-1 逃过 CI 的直接原因。
    for padding in 4085..=4105usize {
        let input = paging_corpus(padding);
        let n = input.chars().count();
        let spans = collect_spans(shared_factories().0.as_ref(), &input);
        let ctx = format!("smart padding={padding}");

        assert_positions_monotonic(&spans, &ctx);
        assert_tail_covered(&spans, n, &ctx);
        assert_full_coverage(&spans, n, &ctx);
    }
}

/// [`test_paging_full_position_coverage`] 的流式路径版本（`segmenter_from_read`）。
#[test]
fn test_paging_full_position_coverage_streaming() {
    use std::io::Cursor;

    for padding in 4085..=4105usize {
        let input = paging_corpus(padding);
        let n = input.chars().count();
        let spans: Vec<(usize, usize, String)> = shared_factories()
            .0
            .segmenter_from_read(Cursor::new(input.into_bytes()))
            .map(|l| {
                (
                    l.begin_position() as usize,
                    l.end_position() as usize,
                    l.text().to_string(),
                )
            })
            .collect();
        let ctx = format!("stream padding={padding}");

        assert_positions_monotonic(&spans, &ctx);
        assert_tail_covered(&spans, n, &ctx);
        assert_full_coverage(&spans, n, &ctx);
    }
}

/// [`test_paging_full_position_coverage`] 的 fine 模式版本。
///
/// fine 模式按设计输出交叉词元，故只断言「不越界 + 覆盖到末尾 + 无空洞」，
/// 不要求位置单调（`assert_positions_monotonic` 不适用）。
#[test]
fn test_paging_full_position_coverage_fine_grained() {
    for padding in 4085..=4105usize {
        let input = paging_corpus(padding);
        let n = input.chars().count();
        let spans = collect_spans(shared_factories().1.as_ref(), &input);
        let ctx = format!("fine padding={padding}");

        assert_tail_covered(&spans, n, &ctx);
        assert_full_coverage(&spans, n, &ctx);
    }
}

#[test]
fn test_word_spanning_buffer_boundary() {
    // 跨缓冲区边界正确性测试：构造 >4096 字输入，验证换页后不重复输出、不丢失字符。
    // 注：跨页词的"完整匹配"受当前双缓冲架构限制（词元坐标在换页时会错位），
    // 待去除双缓冲的重构（评审 #18）解决。本测试聚焦换页正确性（不重不漏）。
    let input = paging_corpus(4090);
    let n = input.chars().count();
    let result_tokens: Vec<String> = shared_factories()
        .0
        .segmenter(&input)
        .map(|l| l.text().to_string())
        .collect();

    // 核心断言：输出字符总数 == 输入字符总数（换页不重不漏）
    let output_chars: usize = result_tokens.iter().map(|t| t.chars().count()).sum();
    assert_eq!(output_chars, n, "换页导致字符重复或丢失");

    // 末尾"万岁"应被正确切分到第二页
    let found_wansui = result_tokens.iter().any(|t| t == "万岁");
    assert!(found_wansui, "'万岁' 未被切出（第二页起始可能丢失）");
}

#[test]
fn test_long_unambiguous_word() {
    let input = "中华人民共和国中央人民政府万岁";

    // --- 1. 智能模式测试 ---
    // 词典含"中华人民共和国"和"中央人民政府"两个词，智能模式切分为两个词
    let expected_smart = "中华人民共和国/中央人民政府/万岁";
    let result_smart = segment_to_string(input, true);
    assert_eq!(
        result_smart, expected_smart,
        "Smart mode failed for long word"
    );

    // --- 2. 细粒度模式测试 ---
    let expected_fine_str = "中华人民共和国/中华人民/中华/华人/人民共和国/人民/共和国/共和/国中/中央人民政府/中央/人民政府/人民/民政/政府/万岁/万/岁";
    let result_fine_str = segment_to_string(input, false);

    // 为了避免因不同语言内部排序导致的问题，我们比较词元的集合而不是有序字符串
    let expected_set = to_token_set(expected_fine_str);
    let result_set = to_token_set(&result_fine_str);

    assert_eq!(
        result_set, expected_set,
        "\nFine-grained mode mismatch! The set of tokens does not match the Java baseline.\n\nActual Result String: {}\n\n",
        result_fine_str
    );
}

#[test]
fn test_sentence_with_all_stop_words() {
    let input = "的了吗啊呀";

    // 预期结果应该是一个空字符串，因为所有词元都被过滤掉了
    let expected = "";

    // let result_smart = segment_to_string(input, true);
    // assert_eq!(result_smart, expected, "Smart mode failed for all-stop-words sentence");

    let result_fine = segment_to_string(input, false);
    assert_eq!(
        result_fine, expected,
        "Fine-grained mode failed for all-stop-words sentence"
    );
}

#[test]
fn test_rare_cjk_characters() {
    // 𪚥 (U+2A6A5), 㱎 (U+3C4E), 曌 (U+66CC, 武则天的名字)
    let input = "罕见字𪚥和曌";

    // 智能模式：长词优先
    let expected_smart = "罕见/字/𪚥/和/曌";
    let result_smart = segment_to_string(input, true);
    assert_eq!(
        result_smart, expected_smart,
        "Smart mode failed for rare CJK characters"
    );

    // 细粒度模式：会额外切出"见"字
    let expected_fine = "罕见/见/字/𪚥/和/曌";
    let result_fine = segment_to_string(input, false);
    assert_eq!(
        result_fine, expected_fine,
        "Fine-grained mode failed for rare CJK characters"
    );
}

// 假设您的 IKSegmenter 在这里

#[test]
fn test_streaming_equals_in_memory() {
    use std::io::Cursor;

    let text = "这是一个中文分词的例子IKAnalyzer2012是一个轻量级的中文分词工具包";
    let factory = shared_factories().0.as_ref();

    // 内存路径
    let mem_result: String = factory
        .segmenter(text)
        .map(|l| l.text().to_string())
        .collect::<Vec<_>>()
        .join("/");

    // 流式路径
    let stream_result: String = factory
        .segmenter_from_read(Cursor::new(text.as_bytes()))
        .map(|l| l.text().to_string())
        .collect::<Vec<_>>()
        .join("/");

    assert_eq!(
        mem_result, stream_result,
        "from_str 与 from_read 产出不一致"
    );
}

#[test]
fn test_streaming_long_text() {
    use std::io::Cursor;

    // padding 从固定 4090 改为扫过换页阈值（评审 P0-1：实测阈值在 4092，
    // 固定 4090 恰好落在阈值之下，是该缺陷逃过 CI 的直接原因）。
    for padding in 4085..=4105usize {
        let input = paging_corpus(padding);
        let n = input.chars().count();
        let factory = shared_factories().0.as_ref();
        let spans: Vec<(usize, usize, String)> = factory
            .segmenter_from_read(Cursor::new(input.into_bytes()))
            .map(|l| {
                (
                    l.begin_position() as usize,
                    l.end_position() as usize,
                    l.text().to_string(),
                )
            })
            .collect();
        let ctx = format!("stream padding={padding}");

        assert_positions_monotonic(&spans, &ctx);
        assert_tail_covered(&spans, n, &ctx);
        assert_full_coverage(&spans, n, &ctx);
    }
}

// ===================================================================
// 与 Kotlin/Java 原版（IK Analyzer 2012_u6）一致性回归测试
// 以下用例均以 Java 原版实际输出为基准（已逐条比对验证）
// ===================================================================

/// 回归：多字量词必须识别为 COUNT / CN_QUAN。
///
/// 曾因 `Dictionary::match_with_hit` 固定在主词典上续接匹配，
/// 导致 `公斤`/`分钟`/`小时`/`年代` 这类「同时存在于主词典与量词词典」的
/// 多字量词被切成 CN_WORD 而非 COUNT，与 Java 原版不一致。
#[test]
fn test_multi_char_quantifier_consistency_with_java() {
    // Java 原版 smart 模式输出：我/买/了/三斤/苹果/和/五公斤/香蕉
    let result = segment_to_string("我买了三斤苹果和五公斤香蕉", true);
    assert!(
        result.contains("五公斤"),
        "多字量词 '五公斤' 未合并, 实际: {}",
        result
    );
    assert!(
        result.contains("三斤"),
        "单字量词 '三斤' 未合并, 实际: {}",
        result
    );
}

/// 回归：数量词需要支持「阿拉伯数字 + 中文数词 + 中文量词」的连续两轮合并。
///
/// Java 原版 `AnalyzeContext.compound` 使用两个**独立的 if**：
/// 第一轮把 ARABIC 合并为 CN_NUM 后，result 类型变为 CN_NUM，
/// 会立即满足第二轮条件继续与 COUNT 合并。
/// 本移植曾误写为 `if / else if`，导致第二轮被短路。
#[test]
fn test_arabic_plus_cnum_plus_count_two_round_compound() {
    // Java 原版 smart 模式：'４２三首' 合并为单个 CN_QUAN 词元
    let result = segment_to_string("唱诗歌４２三首", true);
    assert!(
        result.contains("42三首"),
        "数量词两轮连续合并失败, 期望包含 '42三首', 实际: {}",
        result
    );
}

/// 停用词大小写逃逸（管道级，方案 C' §4 行为矩阵）：
/// 停用词表为小写 "the" 时——
/// - `enable_lowercase=true`：输入 `The` 经 regularize 归一为 `the`，被滤除；
/// - `enable_lowercase=false`（严格 A 语义）：输入保留 `The`，精确匹配查不到 `the`，
///   逃逸过滤被输出。此为 A 语义的固有代价（与 analysis-ik 5.0 一致），
///   使用方需在停用词表中同时收录大小写变体。
#[test]
fn test_stopword_case_escape_semantics() {
    let mk = |lc: bool| {
        TokenizerFactory::new(
            Config::builder()
                .use_smart(true)
                .enable_lowercase(lc)
                .with_stop_words(vec![DictSource::text("the")])
                .build(),
        )
    };
    let tokens = |f: &TokenizerFactory| {
        f.segmenter("The")
            .map(|l| l.text().to_string())
            .collect::<Vec<_>>()
    };

    // true：regularize 小写化 → "the" 被停用词表滤除
    assert!(
        tokens(&mk(true)).is_empty(),
        "enable_lowercase=true 时 'The' 应归一为 'the' 并被滤除"
    );
    // false：保留 "The"，严格匹配查不到 "the" → 逃逸
    assert_eq!(
        tokens(&mk(false)),
        vec!["The".to_string()],
        "enable_lowercase=false 时 'The' 应逃逸停用词过滤被保留"
    );
}

// =====================================================================
// 默认配置与 tag v1.0.0（IK Analyzer 2012_u6）的行为等价性
//
// v1.0.0 的真实语义 = `use_smart(true)` + `enable_lowercase(true)`
// （输入与词典均做 ASCII 大小写归一，输入侧另做全角→半角；两版 `regularize` 代码同源）。
// 本仓库当前默认值即此语义，下列测试锁定之。
//
// 与 v1.0.0 的**唯一有意偏差**：默认停用词表非空
// （v1.0.0 的 `dict/ext_stop_word/stop.dic` 是 0 字节，停用词功能未生效）。
// 其余差异均为 v1.0.0 自身缺陷的修复（`compound` 的 `if/else if` → 两个独立 `if`、
// 缓冲区换页重复输出），详见 docs/使用手册.md「与 v1.0.0 的行为等价性」。
// =====================================================================

fn tokens_of(factory: &TokenizerFactory, text: &str) -> Vec<String> {
    factory
        .segmenter(text)
        .map(|l| l.text().to_string())
        .collect()
}

/// 默认配置 == 显式的 v1.0.0 语义（smart + lowercase）。
#[test]
fn test_default_config_equals_v1_0_0_profile() {
    let default_factory = TokenizerFactory::new(Config::builder().build());
    let v1_factory = TokenizerFactory::new(
        Config::builder()
            .use_smart(true)
            .enable_lowercase(true)
            .build(),
    );

    let inputs = [
        "IKAnalyzer2012是一个轻量级的中文分词工具包",
        "张三说的确实在理",
        "研究生命起源",
        "唱诗歌４２三首",
        "欢迎使用本分词器",
    ];
    for text in inputs {
        assert_eq!(
            tokens_of(&default_factory, text),
            tokens_of(&v1_factory, text),
            "默认配置应与 v1.0.0 语义等价（输入：{text}）"
        );
    }
}

/// 默认 `use_smart = true`（v1.0.0 `ConfigBuilder::new()` 的默认值，即 smart 而非细粒度）。
#[test]
fn test_default_use_smart_is_true() {
    let text = "张三说的确实在理";
    let factory = TokenizerFactory::new(Config::builder().build());
    let default = tokens_of(&factory, text).join("/");
    assert_eq!(
        default,
        segment_to_string(text, true),
        "默认应为 smart 模式"
    );
    assert_ne!(
        default,
        segment_to_string(text, false),
        "默认不应是细粒度模式"
    );
}

/// 默认大小写不敏感（与 v1.0.0 一致）：ASCII 大写输入归一为小写输出。
#[test]
fn test_default_case_insensitive() {
    let factory = TokenizerFactory::new(Config::builder().build());
    assert_eq!(
        tokens_of(&factory, "IKAnalyzer2012中文分词"),
        tokens_of(&factory, "ikanalyzer2012中文分词"),
        "大写/小写输入应产出相同词元"
    );
    assert!(
        tokens_of(&factory, "HelloWorld").contains(&"helloworld".to_string()),
        "默认应把小写化后的字母 token 输出"
    );
}

/// 默认全角→半角归一 + else-if 大小写语义（与 u6 `CharacterUtil.java:89-101` 一致）。
///
/// **关键语义（决策 3 修复后与 u6 逐字一致）**：`regularize` 是 else-if 链，
/// 全角分支命中后**不再进入**小写分支，故全角大写只降宽度、不叠加小写：
/// - `ＡＢＣ` → `ABC`（保留大写），而 `ABC` → `abc`（半角走小写分支）；
/// - 因此全角/半角**字母**输入产出**不同**词元——这是 u6 的真实行为，
///   历史上的「顺序式」实现（`Ａ`→`a`）是双方参照都没有的第三种行为；
/// - 数字/标点不受影响，全角/半角仍等价。
#[test]
fn test_default_fullwidth_halfwidth_equivalent() {
    let factory = TokenizerFactory::new(Config::builder().build());

    // 数字与标点：全角/半角等价（不受 else-if 大小写语义影响）
    for (full_width, half_width) in [("１２３公斤", "123公斤"), ("１，２。", "1,2.")] {
        assert_eq!(
            tokens_of(&factory, full_width),
            tokens_of(&factory, half_width),
            "全角/半角数字标点应产出相同词元：{full_width} vs {half_width}"
        );
    }

    // 字母：全角只降宽度保留大写，半角走小写分支——与 u6 一致地**不**等价
    assert_eq!(
        tokens_of(&factory, "ＡＢＣ"),
        vec!["ABC".to_string()],
        "全角大写应只降宽度为 'ABC'（else-if 语义，不叠加小写）"
    );
    assert_eq!(
        tokens_of(&factory, "ABC"),
        vec!["abc".to_string()],
        "半角大写应走小写分支归一为 'abc'"
    );
    assert_eq!(
        tokens_of(&factory, "ＩＫＡＮＡＬＹＺＥＲ"),
        vec!["IKANALYZER".to_string()],
        "全角字母串应降宽度并保留大写"
    );
    assert_eq!(
        tokens_of(&factory, "ＱＱ宠物"),
        vec!["QQ".to_string(), "宠物".to_string()],
        "全角 QQ 保留大写、不命中（折叠为小写的）词典词条"
    );
    assert_eq!(
        tokens_of(&factory, "QQ宠物"),
        vec!["qq宠物".to_string()],
        "半角 QQ 归一为小写后命中词典词条"
    );
}

/// 默认下词典与输入同口径归一（构建期策略，方案 C'）。
///
/// 决策 2（CJK 门控修复）后，含 ASCII 的 CJK 词条端到端**可观测**：
/// `A股` 输入折叠为 `a股` 命中词典（u6 行为）；修复前该词条被门控挡死。
#[test]
fn test_default_dict_and_input_same_case_folding() {
    // 默认（lowercase=true）：输入大写 "THE" 归一为 "the" → 与词表精确命中 → 滤除
    let default = TokenizerFactory::new(Config::builder().build());
    assert!(
        tokens_of(&default, "THE").is_empty(),
        "默认应大小写不敏感：'THE' 归一为 'the' 后被停用词表滤除"
    );

    // 决策 2：含 ASCII 的词典词条端到端可达（`A股` → `a股`，= u6）
    assert_eq!(
        tokens_of(&default, "A股"),
        vec!["a股".to_string()],
        "CJK 门控修复后 'A股' 应折叠命中词典词条（u6 行为）"
    );

    // 严格 A 语义（lowercase=false）：输入保留 "THE"、词典保留 "the" → 不命中 → 逃逸
    let strict = TokenizerFactory::new(
        Config::builder()
            .use_smart(true)
            .enable_lowercase(false)
            .build(),
    );
    assert_eq!(
        tokens_of(&strict, "THE"),
        vec!["THE".to_string()],
        "lowercase=false 时 'THE' ≠ 'the'，应逃逸停用词过滤"
    );
}

// =====================================================================
// 零拷贝入口 segment_spans（评审 P2-1）
//
// `segmenter()` 为每个输出词元做一次 `String` 分配，实测占分词热路径全部堆
// 分配的 94%。`segment_spans()` 产出只含位置与类型的 `Span`，由调用方自行
// 从原文切片。本组用例锁定：**两条路径的词元序列必须完全一致**。
// =====================================================================

/// `segment_spans` 与 `segmenter` 产出完全一致的词元**位置与类型**序列。
///
/// 注意二者**文本**的语义本就不同（不是缺陷）：
/// - `Lexeme::text()` 是**归一化后**的文本（全角→半角 + 可选 ASCII 小写化）；
/// - `Span::slice(source, map)` 切的是**原文**，不做归一化。
///
/// 故对含大写 ASCII / 全角的输入，`text()` 为 `ikanalyzer2012` 而切片为
/// `IKAnalyzer2012`；**位置与类型不受影响**。
#[test]
fn test_segment_spans_matches_segmenter() {
    let factory = TokenizerFactory::new(Config::builder().use_smart(true).build());
    for text in [
        "欢迎使用本分词器",
        "这是一个中文分词的例子",
        "IKAnalyzer2012是一个轻量级的中文分词工具包",
        "研究生命起源",
        "唱诗歌４２三首",
        "我买了三斤苹果和五公斤香蕉",
        "的了吗啊呀", // 全部为停用词 → 空序列
        "HelloWorld 中文分词",
        "QQ宠物与A股上涨",
        "",
    ] {
        let with_text: Vec<(usize, usize, i32)> = factory
            .segmenter(text)
            .map(|l| {
                (
                    l.begin_position() as usize,
                    l.end_position() as usize,
                    l.lexeme_type(),
                )
            })
            .collect();

        let spans: Vec<(usize, usize, i32)> = factory
            .segment_spans(text)
            .map(|s| (s.begin(), s.end(), s.lexeme_type()))
            .collect();

        assert_eq!(
            spans, with_text,
            "segment_spans 与 segmenter 的位置/类型序列不一致，输入：{text:?}"
        );

        // 切片恒等于原文按 char 区间的取子串（与是否归一化无关）
        let map = char_to_byte_map(text);
        for s in factory.segment_spans(text) {
            let expected: String = text.chars().skip(s.begin()).take(s.length()).collect();
            assert_eq!(
                s.slice(text, &map),
                expected,
                "切片应为原文子串，输入：{text:?} span={:?}",
                (s.begin(), s.end())
            );
        }
    }

    // 无归一化影响时，切片文本与 Lexeme::text() 逐字相同
    let plain = "欢迎使用本分词器";
    let map = char_to_byte_map(plain);
    let from_span: Vec<String> = factory
        .segment_spans(plain)
        .map(|s| s.slice(plain, &map).to_string())
        .collect();
    assert_eq!(from_span, tokens_of(&factory, plain));
}

/// `Span` 切片在多字节文本与跨缓冲页文本上都正确还原原文。
#[test]
fn test_span_slice_round_trip() {
    let factory = TokenizerFactory::new(Config::builder().use_smart(true).build());

    // 多字节混合文本
    let text = "中文abc混合🎉文本";
    let map = char_to_byte_map(text);
    for s in factory.segment_spans(text) {
        let slice = s.slice(text, &map);
        // 切片内容应与按 char 区间取出的内容一致
        let expected: String = text.chars().skip(s.begin()).take(s.length()).collect();
        assert_eq!(
            slice,
            expected,
            "span [{}, {}) 切片内容不符",
            s.begin(),
            s.end()
        );
        // 且切片在原文中的字节位置正确
        assert_eq!(
            slice.as_ptr() as usize - text.as_ptr() as usize,
            map[s.begin()] as usize
        );
    }

    // 跨页文本（触发换页）：切片仍应还原正确原文
    let long = format!("{}中华人民共和国万岁", "占".repeat(4090));
    let map = char_to_byte_map(&long);
    let mut count = 0;
    for s in factory.segment_spans(&long) {
        let slice = s.slice(&long, &map);
        let expected: String = long.chars().skip(s.begin()).take(s.length()).collect();
        assert_eq!(
            slice,
            expected,
            "跨页 span [{}, {}) 切片内容不符",
            s.begin(),
            s.end()
        );
        count += 1;
    }
    assert!(count > 0, "跨页文本应产出 span");
}

/// `Span` 的坐标连续、无重叠，且覆盖到末尾（对齐 P0-1 建立的不变式）。
#[test]
fn test_span_positions_are_contiguous() {
    let factory = TokenizerFactory::new(Config::builder().use_smart(true).build());
    let input = paging_corpus(4092); // 换页阈值附近
    let n = input.chars().count();
    let spans = collect_spans(&factory, &input);

    let mut prev_end = 0usize;
    for (b, e, text) in &spans {
        assert_eq!(
            *b,
            prev_end,
            "span 位置应连续（{}:{}）",
            text,
            input.chars().skip(*b).take(e - b).collect::<String>()
        );
        prev_end = *e;
    }
    assert_eq!(prev_end, n, "span 序列应覆盖到输入末尾");

    // 与 segment_spans 交叉验证
    let spans2: Vec<(usize, usize)> = factory
        .segment_spans(&input)
        .map(|s| (s.begin(), s.end()))
        .collect();
    let spans1: Vec<(usize, usize)> = spans.iter().map(|(b, e, _)| (*b, *e)).collect();
    assert_eq!(spans1, spans2, "segment_spans 与 segmenter 位置序列不一致");
}

/// `char_to_byte_map` 的边界与自洽性。
#[test]
fn test_char_to_byte_map_shape() {
    for text in ["", "a", "中文", "abc", "中文abc", "🎉🎉", "中a🎉文"] {
        let m = char_to_byte_map(text);
        assert_eq!(
            m.len(),
            text.chars().count() + 1,
            "映射表长度应为 字符数+1（末项供切片到结尾），文本：{text:?}"
        );
        assert_eq!(m[0], 0, "首项应为 0");
        assert_eq!(
            m.last().copied().unwrap() as usize,
            text.len(),
            "末项应为 text.len()（byte 长度）"
        );
        assert!(
            m.windows(2).all(|w| w[0] <= w[1]),
            "映射表应单调不减：{text:?}"
        );
        // 按映射表逐段切片，应能无损拼回原文
        let mut rebuilt = String::new();
        for i in 0..text.chars().count() {
            rebuilt.push_str(&text[m[i] as usize..m[i + 1] as usize]);
        }
        assert_eq!(rebuilt, text, "按映射表逐段拼接应还原原文：{text:?}");
    }
}

// =====================================================================
// 停用词过滤的整段语义护栏（评审 P1-1 修正后的固化）
//
// 本项最初被误判为"与 IK 原版 matchText 语义偏离"，实施按"路径上任一位置成词即
// match"改写后，分词输出立即回归（smart 202231→202215，fine 161328→159815）。
// 根因：`is_stop_word` 的调用点传入**词元长度**（length > 1），改成前缀语义后，
// 任何"前缀命中停用词"的长词元都会被静默误删。
//
// 结论：当前的**整段成词**语义是与 IK 原版 2012_u6 对齐后的既定行为（已由
// 9133 行语料 × 4 口径对 u6 三方差分、0 分歧验证），**不是缺陷**。
// 本组用例把这条不变量固化下来，防止再次被"优化"掉。
// =====================================================================

/// 停用词判定必须是**整段成词**，不能是"路径上任一位置成词即命中"。
#[test]
fn test_stop_word_filter_is_whole_token_not_prefix() {
    let factory = TokenizerFactory::new(
        Config::builder()
            .with_main_dict(vec![DictSource::text("中国\n中止")])
            .with_stop_words(vec![DictSource::text("中")])
            .build(),
    );

    // 前缀 "中" 是停用词，但整段 "中国" 不是 → 必须保留
    let tokens = tokens_of(&factory, "中国");
    assert_eq!(
        tokens,
        vec!["中国".to_string()],
        "\"中国\" 的前缀 '中' 命中停用词，但整段不是停用词，不应被过滤（前缀语义会误删）"
    );

    // 同理："中止" 的前缀 "中" 命中停用词，整段不是 → 必须保留
    let tokens = tokens_of(&factory, "中止");
    assert_eq!(
        tokens,
        vec!["中止".to_string()],
        "\"中止\" 不应因前缀 '中' 命中停用词而被过滤"
    );

    // 整段就是停用词 → 过滤（停用词功能本身仍须生效，不能被护栏测试锁死）
    let tokens = tokens_of(&factory, "中");
    assert!(
        tokens.is_empty(),
        "整段 '中' 本身就是停用词，应被过滤；实际: {tokens:?}"
    );
}

/// `match_in_main_dict` 在 `length > 1` 时的整段语义（与 `is_stop_word` 配套）。
///
/// 记住所测语义：**整段成词才 match**。`"中国"` 在词典 {"中国","中止"} 下整段成词；
/// `"中国人"` 因末字断链而**不**成词（但前缀 "中国" 成词 —— 整段语义不看前缀）。
#[test]
fn test_match_text_multi_char_whole_token_semantics() {
    let factory = TokenizerFactory::new(
        Config::builder()
            .with_main_dict(vec![DictSource::text("中国\n中止")])
            .build(),
    );
    let dict = factory.dictionary();

    let chars: Vec<char> = "中国".chars().collect();
    let h = dict.match_in_main_dict(&chars, 0, 2);
    assert!(h.is_match(), "整段 '中国' 成词");
    assert_eq!(h.end(), 2, "整段走完，end = begin + length");

    // 末字断链 → 整段语义下**不** match（前缀 '中国' 成词不算）
    let chars3: Vec<char> = "中国人".chars().collect();
    let h = dict.match_in_main_dict(&chars3, 0, 3);
    assert!(
        !h.is_match(),
        "整段 '中国人' 因末字断链不成词；前缀成词不应影响整段判定"
    );
    assert_eq!(h.end(), 0, "中途失配即返回未置位 hit，end 保持 0");

    // 越界长度自动截断，不 panic
    let short: Vec<char> = "中国".chars().collect();
    let h = dict.match_in_main_dict(&short, 1, 99);
    assert!(!h.is_match() && !h.is_prefix());
    assert_eq!(h.end(), 0, "'国' 不在词典中，首字符即失配");
}

// =====================================================================
// 流式 IO 错误的可观测性（评审 P0-2）
//
// `Iterator<Item = Lexeme>` 无法返回错误，故读取失败时本库只能"停止并返回
// 已读到的部分"，即内容被截断。20260930 的 P0-5 修复让错误不再被完全吞掉，
// 但只落 `log::error!`——而 `env_logger` 已是 optional feature，下游默认无任何
// 输出，表现为"静默截断"。本组用例锁定 `IKSegmenter::take_error()` 这一出口。
// =====================================================================

/// 先产出若干合法字节，随后返回 IO 错误的 `Read` 实现。
struct FailingReader {
    /// 已成功返回的前缀
    prefix: Vec<u8>,
    /// 已返回的字节数
    served: usize,
    /// 报错前额外返回的合法字节数（用于验证"错误前的部分仍被正确分词"）
    trailing: Vec<u8>,
    /// 已返回的 trailing 字节数
    served_trailing: usize,
    /// 在此次 read 上返回的错误
    error: std::io::Error,
}

impl std::io::Read for FailingReader {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        if self.served < self.prefix.len() {
            let n = self.prefix.len().min(self.served + buf.len());
            let chunk = &self.prefix[self.served..n];
            buf[..chunk.len()].copy_from_slice(chunk);
            self.served = n;
            return Ok(chunk.len());
        }
        if self.served_trailing < self.trailing.len() {
            let n = self.trailing.len().min(self.served_trailing + buf.len());
            let chunk = &self.trailing[self.served_trailing..n];
            buf[..chunk.len()].copy_from_slice(chunk);
            self.served_trailing = n;
            return Ok(chunk.len());
        }
        Err(std::io::Error::other(self.error.to_string()))
    }
}

/// 流式读取中途失败时，`take_error()` 必须能取到错误，且错误前的部分仍被完整分词。
#[test]
fn test_streaming_io_error_is_observable() {
    let factory = shared_factories().0.as_ref();
    let prefix = "这是一个中文分词的例子".as_bytes().to_vec();
    let mut seg = factory.segmenter_from_read(FailingReader {
        prefix,
        served: 0,
        trailing: Vec::new(),
        served_trailing: 0,
        error: std::io::Error::other("injected read failure"),
    });

    // 错误前的部分照常分词
    let tokens: Vec<String> = seg.by_ref().map(|l| l.text().to_string()).collect();
    assert_eq!(
        tokens.join("/"),
        segment_to_string("这是一个中文分词的例子", true),
        "读取失败前的部分应被正确分词"
    );

    // 关键断言：错误可被调用方观测（而非只落日志）
    assert!(seg.has_error(), "流式读取失败后 has_error() 应为 true");
    let err = seg
        .take_error()
        .expect("take_error() 应取回 IO 错误（内容可能被截断）");
    assert!(
        err.to_string().contains("injected read failure"),
        "取回的错误应保留原始信息，实际: {err}"
    );
    // take 语义：取走后不再重复报告
    assert!(!seg.has_error(), "take_error() 取走后应清空");
    assert!(seg.take_error().is_none(), "take_error() 应为 take 语义");
}

/// 内存路径恒不产生 IO 错误（`take_error()` 恒为 `None`）。
#[test]
fn test_in_memory_path_never_reports_io_error() {
    let factory = shared_factories().0.as_ref();
    let mut seg = factory.segmenter("中文分词正常");
    let _: Vec<String> = seg.by_ref().map(|l| l.text().to_string()).collect();
    assert!(
        !seg.has_error() && seg.take_error().is_none(),
        "内存路径不应产生 IO 错误"
    );
}

/// 读取完全正常时不得误报错误。
#[test]
fn test_streaming_without_error_reports_none() {
    use std::io::Cursor;
    let factory = shared_factories().0.as_ref();
    let mut seg =
        factory.segmenter_from_read(Cursor::new("这是一个中文分词的例子".as_bytes().to_vec()));
    let tokens: Vec<String> = seg.by_ref().map(|l| l.text().to_string()).collect();
    assert!(!tokens.is_empty());
    assert!(seg.take_error().is_none(), "正常读取不应报告 IO 错误");
}

/// 版本一致性：`VERSION` 常量必须与 `Cargo.toml` 声明一致（评审 P3-1）。
///
/// 版本改由 `[workspace.package]` 单一声明，本测试确保 `pub const VERSION`
/// 确实随包版本更新——它是 Python 绑定 `__version__` 的数据源，若失准会
/// 再次出现"报告的版本与实际链接的核心库不符"。
#[test]
fn test_version_constant_is_in_sync() {
    // `CARGO_PKG_VERSION` 由 Cargo 在编译期注入，与本 crate 的 package version 同源。
    assert_eq!(
        ik_analyzer::VERSION,
        env!("CARGO_PKG_VERSION"),
        "VERSION 常量应等于 CARGO_PKG_VERSION"
    );
    // 形态校验：x.y.z，非空且无前后空白
    let parts: Vec<&str> = ik_analyzer::VERSION.split('.').collect();
    assert!(
        parts.len() >= 3 && parts.iter().all(|p| p.chars().all(|c| c.is_ascii_digit())),
        "VERSION 应形如 x.y.z，实际 {:?}",
        ik_analyzer::VERSION
    );
}

/// 决策 10.5：内置停用词表的英文部分 = u6 分发口径（与 u6 `stopword.dic`
/// 逐字节相同的 33 个词），且该口径下停用词过滤机制与 u6 一致。
///
/// 意义：parity 契约配置映射表的第二行（u6 jar 分发装配）由此可一行复现——
/// `with_stop_words(vec![DictSource::text(u6 stopword.dic 内容)])`，而
/// 内置表的前 33 行正是该内容（披露见 README / 使用手册 §6.4）。
#[test]
fn test_builtin_english_stopwords_match_u6_distribution() {
    // u6 `src/stopword.dic` 全表（33 词，2026-10-12 与
    // refs/IK-Analyzer-2012_u6_source/src/stopword.dic 逐字节 diff 验证）
    const U6_STOPWORD_DIC: &str = "a\nan\nand\nare\nas\nat\nbe\nbut\nby\nfor\nif\nin\ninto\nis\nit\nno\nnot\nof\non\nor\nsuch\nthat\nthe\ntheir\nthen\nthere\nthese\nthey\nthis\nto\nwas\nwill\nwith";

    // 内置停用词表的前 33 行必须与之相同（锁定披露声明）
    let builtin = include_str!("../dict/ext_stop_word/stop.dic");
    let builtin_lines: Vec<&str> = builtin
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .collect();
    assert!(
        builtin_lines.len() >= 33,
        "内置停用词表应至少 33 行，实际 {}",
        builtin_lines.len()
    );
    assert_eq!(
        &builtin_lines[..33],
        &U6_STOPWORD_DIC.lines().collect::<Vec<_>>()[..],
        "内置停用词表前 33 行应与 u6 stopword.dic 逐字节相同（披露声明锁定）"
    );

    // 机制验证：以该 33 词表显式配置（u6 jar 分发口径），过滤行为与 u6 一致
    let factory = TokenizerFactory::new(
        Config::builder()
            .use_smart(true)
            .with_stop_words(vec![DictSource::text(U6_STOPWORD_DIC)])
            .build(),
    );
    for word in ["a", "the", "is", "with", "an", "these"] {
        assert!(
            tokens_of(&factory, word).is_empty(),
            "u6 分发口径下 '{word}' 应被停用词过滤"
        );
    }
    // 非停用词的普通英文词不受影响
    assert_eq!(
        tokens_of(&factory, "cat"),
        vec!["cat".to_string()],
        "非停用词不应被过滤"
    );
}

// =====================================================================
// 目标重新对齐新增用例
//
// 决策 2：CJK 门控修复（== CHAR_USELESS 才跳过）——含 ASCII 词条端到端可达；
// 决策 3：regularize else-if——全角大写不叠加小写；
// 决策 4：AstralMode 三值枚举；
// 决策 1：Config::u6_parity 显式 parity 预设。
// =====================================================================

/// 决策 2：CJK 门控修为「非 USELESS 即推进」后，内置 9 个含 ASCII 词条
/// （`A股`/`B股`/`AB股`/`H股`/`K线`/`U盘`/`QQ宠物`/`QQ飞车`/`Hold住`）
/// 端到端可达（此前被门控挡死，是词典层测试覆盖不到的盲区）。
#[test]
fn test_cjk_gate_ascii_dict_words_end_to_end() {
    let factory = TokenizerFactory::new(Config::builder().use_smart(true).build());
    for (input, expected) in [
        ("A股", "a股"),
        ("B股", "b股"),
        ("AB股", "ab股"),
        ("H股", "h股"),
        ("K线", "k线"),
        ("U盘", "u盘"),
        ("QQ宠物", "qq宠物"),
        ("QQ飞车", "qq飞车"),
        ("Hold住", "hold住"),
    ] {
        assert_eq!(
            tokens_of(&factory, input),
            vec![expected.to_string()],
            "内置混合词条 '{input}' 应折叠命中词典（u6 行为），门控回归？"
        );
    }

    // 用户自定义混合词条同样可达（中文前缀 + ASCII 后缀）
    let custom = TokenizerFactory::new(
        Config::builder()
            .use_smart(true)
            .add_ext_dict_text("阿3\nX光片")
            .build(),
    );
    assert_eq!(
        tokens_of(&custom, "阿3"),
        vec!["阿3".to_string()],
        "用户混合词条 '阿3' 应可达"
    );
    assert_eq!(
        tokens_of(&custom, "X光片"),
        vec!["x光片".to_string()],
        "用户混合词条 'X光片' 应折叠命中"
    );
}

/// 决策 3：全角大写 `Ａ` 只降宽度为 `A`、不叠加小写（u6 else-if 语义），
/// 故 `Ａ股` 不命中（折叠为小写的）词典词条，输出 `A/股`——与 u6 一致。
#[test]
fn test_fullwidth_uppercase_else_if_semantics() {
    let factory = TokenizerFactory::new(Config::builder().use_smart(true).build());
    assert_eq!(
        tokens_of(&factory, "Ａ股"),
        vec!["A".to_string(), "股".to_string()],
        "全角 Ａ 只降宽度保留大写，不命中小写词典词条（u6 行为）"
    );
    assert_eq!(
        tokens_of(&factory, "ＡＢＣ"),
        vec!["ABC".to_string()],
        "全角字母串保留大写"
    );
}

/// 决策 4：`AstralMode` 三值枚举的端到端行为。
#[test]
fn test_astral_modes() {
    let mk = |mode: AstralMode| {
        TokenizerFactory::new(Config::builder().use_smart(true).astral_mode(mode).build())
    };

    // Drop（= u6 严格口径）：全部非 BMP 丢弃，含扩展区汉字
    let drop = mk(AstralMode::Drop);
    assert_eq!(
        tokens_of(&drop, "你好😀世界"),
        vec!["你好".to_string(), "世界".to_string()],
        "Drop 应丢弃 emoji（= u6）"
    );
    assert_eq!(
        tokens_of(&drop, "中𪚥文"),
        vec!["中".to_string(), "文".to_string()],
        "Drop 应丢弃扩展区汉字（= u6 严格口径）"
    );

    // Ideographs（默认）：扩展区汉字识别为汉字，emoji 丢弃
    let ideo = mk(AstralMode::Ideographs);
    assert_eq!(
        tokens_of(&ideo, "中𪚥文"),
        vec!["中".to_string(), "𪚥".to_string(), "文".to_string()],
        "Ideographs 应识别扩展区汉字"
    );
    assert_eq!(
        tokens_of(&ideo, "你好😀世界"),
        vec!["你好".to_string(), "世界".to_string()],
        "Ideographs 仍丢弃 emoji"
    );

    // Tokens：全部非 BMP 输出为单字 CNCHAR 词元（机制对齐 analysis-ik 目标）
    let tokens_mode = mk(AstralMode::Tokens);
    assert_eq!(
        tokens_of(&tokens_mode, "你好😀世界"),
        vec!["你好".to_string(), "😀".to_string(), "世界".to_string()],
        "Tokens 应把 emoji 输出为词元"
    );
    // 不照搬 B 的缺陷：紧邻 astral 的单字母 ENGLISH 不丢失
    // （默认停用词含 "a"，故用 "x"/"y" 验证——B 在此处会丢紧邻的单字母）
    assert_eq!(
        tokens_of(&tokens_mode, "x😀y"),
        vec!["x".to_string(), "😀".to_string(), "y".to_string()],
        "Tokens 模式下紧邻 astral 的单字母不应丢失（B 的 SurrogatePairSegmenter 缺陷）"
    );
    assert_eq!(
        tokens_of(&tokens_mode, "中𪚥文"),
        vec!["中".to_string(), "𪚥".to_string(), "文".to_string()],
        "Tokens 下扩展区汉字行为同 Ideographs"
    );
}

/// 决策 4 + 缓冲区：astral 字符跨 4096 换页不得被拆断（B 的
/// `SurrogatePairSegmenter` 正是在页尾把代理对拆成半个；Rust `char` 模型
/// 天然规避，此处以探针固化）。
#[test]
fn test_astral_across_buffer_boundary() {
    // 汉字流 + emoji：emoji 落在第 4096 个字符处（换页临界区）
    let mut input: String = "中".repeat(4095);
    input.push('😀');
    input.push_str("测试文本");
    let n = input.chars().count();
    let emoji_idx = 4095usize;

    // Tokens：emoji 应作为**完整**词元出现（长度 1、文本可回读、位置正确），
    // 而非被拆成无法回读的孤立代理（B 的缺陷形态）
    let factory = TokenizerFactory::new(
        Config::builder()
            .use_smart(true)
            .astral_mode(AstralMode::Tokens)
            .build(),
    );
    let spans = collect_spans(&factory, &input);
    assert_tail_covered(&spans, n, "Tokens astral 跨页");
    assert_positions_monotonic(&spans, "Tokens astral 跨页");
    let emoji_spans: Vec<&(usize, usize, String)> =
        spans.iter().filter(|(_, _, t)| t == "😀").collect();
    assert_eq!(
        emoji_spans.len(),
        1,
        "跨页 emoji 应恰成一个完整词元，实际: {spans:?}"
    );
    let (b, e, _) = emoji_spans[0];
    assert_eq!((*b, *e), (emoji_idx, emoji_idx + 1), "emoji 词元位置应正确");

    // Ideographs：emoji 被丢弃（= u6）。USELESS 字符在偏移空间留空洞是
    // u6 的既定行为（调研报告 §5.1，两侧一致），但除该位置外不得有覆盖空洞。
    let factory = TokenizerFactory::new(
        Config::builder()
            .use_smart(true)
            .astral_mode(AstralMode::Ideographs)
            .build(),
    );
    let spans = collect_spans(&factory, &input);
    assert_tail_covered(&spans, n, "Ideographs astral 跨页");
    assert!(
        !spans.iter().any(|(_, _, t)| t == "😀"),
        "Ideographs 应丢弃 emoji"
    );
    let mut covered = vec![false; n];
    for (b, e, _) in &spans {
        for slot in covered.iter_mut().take(*e).skip(*b) {
            *slot = true;
        }
    }
    let holes: Vec<usize> = covered
        .iter()
        .enumerate()
        .filter(|(i, c)| !**c && *i != emoji_idx)
        .map(|(i, _)| i)
        .collect();
    assert!(
        holes.is_empty(),
        "除 emoji 位置外不应有覆盖空洞，空洞: {holes:?}"
    );
}

/// 决策 1：`Config::u6_parity` 显式 parity 预设——停用词置空、astral 丢弃、
/// 大小写不敏感，其余与默认一致（u6 bare core 口径）。
#[test]
fn test_u6_parity_preset() {
    for use_smart in [true, false] {
        let parity = TokenizerFactory::new(Config::u6_parity(use_smart));
        let default = TokenizerFactory::new(Config::builder().use_smart(use_smart).build());

        // 停用词置空：「的了吗」不再被过滤（默认配置会过滤）
        assert_eq!(
            tokens_of(&parity, "的了吗"),
            vec!["的".to_string(), "了吗".to_string()],
            "u6_parity(smart={use_smart}) 不应过滤停用词"
        );
        assert!(
            tokens_of(&default, "的了吗").iter().all(|t| t != "的"),
            "默认配置应过滤 '的'（对照项）"
        );

        // astral 丢弃：扩展区汉字与 emoji 均不输出（= u6）
        assert_eq!(
            tokens_of(&parity, "中𪚥文😀"),
            vec!["中".to_string(), "文".to_string()],
            "u6_parity(smart={use_smart}) 应丢弃全部非 BMP 字符"
        );

        // 大小写不敏感（= u6）：A股 → a股（smart 精确输出；fine 另有交叉子词元）
        let parity_tokens = tokens_of(&parity, "A股");
        assert!(
            parity_tokens.contains(&"a股".to_string()),
            "u6_parity(smart={use_smart}) 应大小写不敏感命中 'a股'，实际: {parity_tokens:?}"
        );
        if use_smart {
            assert_eq!(
                parity_tokens,
                vec!["a股".to_string()],
                "smart 模式应精确输出 'a股'"
            );
        }

        // 全角大写 else-if 语义（= u6）：Ａ股 → A/股
        assert_eq!(
            tokens_of(&parity, "Ａ股"),
            vec!["A".to_string(), "股".to_string()],
            "u6_parity(smart={use_smart}) 全角大写应保留大写（u6 行为）"
        );
    }
}

// =====================================================================
// 「XY村」村名切分与 u6 一致（issue 20261012-CJK词典词条仲裁候选缺失）
//
// 根因：词典行清洗曾用 `str::trim()`（Unicode White_Space 全集），而 u6 的
// `String.trim()` 只裁码点 ≤ U+0020。geocoding region.dic:249714 的词条是
// `\u{3000}村`（U+3000 前缀污染行）——Rust 裁成 `村` 后它进入仲裁候选集，
// `深潭村` 被切 `[深潭, 村]`，u6（`村` 不成词）为 `[深, 潭村]`。
// 修复：裁剪集收紧为「≤U+0020 + NBSP」（NBSP 裁剪是有意改进，见
// `src/dic/dictionary.rs::clean_dict_word` 文档）。
// 验证：10000 行真实地址语料对 Java u6 直驱 jar 全量对拍，分歧行 25 → 1
// （残余 1 行即 issue §7.4 声明保留的 `方正凤凰城` NBSP 差异）。
// =====================================================================

/// 词典含 U+3000 前缀污染行时，`村` 不得成词，"XY村"村名切分与 u6 一致。
#[test]
fn test_village_name_split_matches_u6_with_u3000_polluted_dict_line() {
    // 复刻 geocoding 词典事实：深潭/洄源 ∈ main2012.dic；潭村/源村 ∈ region.dic
    // 整词；region.dic:249714 = `\u{3000}村`（U+3000 前缀，u6 不裁）。
    let factory = TokenizerFactory::new(
        Config::builder()
            .use_smart(true)
            .with_main_dict(vec![DictSource::text(
                "深潭\n潭村\n\u{3000}村\n洄源\n源村\n",
            )])
            .build(),
    );

    // u6 直驱实测（ikanalyzer-2012_u6.jar + 相同扩展词典）：
    //   深潭村 → [深, 潭村]；洄源村 → [洄, 源村]
    assert_eq!(
        tokens_of(&factory, "深潭村"),
        vec!["深".to_string(), "潭村".to_string()],
        "深潭村 应切 [深, 潭村]（u6 行为）；污染行不得裁出独立词 '村'"
    );
    assert_eq!(
        tokens_of(&factory, "洄源村"),
        vec!["洄".to_string(), "源村".to_string()],
        "洄源村 应切 [洄, 源村]（u6 行为）"
    );
}

/// NBSP（U+00A0）尾巴词条仍被裁剪、保持可命中——issue §7.4 声明保留的有意改进
/// （`方正凤凰城\u{a0}{4}`，community.dic:169476；u6 不可命中，Rust 可命中）。
#[test]
fn test_nbsp_polluted_dict_line_stays_matchable() {
    let factory = TokenizerFactory::new(
        Config::builder()
            .use_smart(true)
            .with_main_dict(vec![DictSource::text(
                "方正凤凰城\u{00A0}\u{00A0}\u{00A0}\u{00A0}",
            )])
            .build(),
    );
    assert_eq!(
        tokens_of(&factory, "方正凤凰城"),
        vec!["方正凤凰城".to_string()],
        "NBSP 尾巴应被裁掉，词条保持可命中（有意改进，勿回退）"
    );
}
