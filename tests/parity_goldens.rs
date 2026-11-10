//! 长文（>4096 字符）parity golden 回归测试。
//!
//! # 背景与基线来源（设计决策 10）
//!
//! 9133 行对账语料最长 646 字符，**结构上不可能覆盖 >4096 换页路径**——
//! 所有既有「0 分歧」结论都不蕴含长文正确性。本组用例以 golden-file 方式
//! 把长文差分纳入 CI（CI 无 Java，golden 由 `testing/parity/regen_goldens.sh`
//! 在本地生成）。
//!
//! **基线为什么取自 analysis-ik 而非 u6 jar**：u6 每页边界重复输出 1 个
//! 重叠 token（off-by-one，A 级实测，调研报告 §9.1；例外清单 #1），>4096
//! 输入必然分歧；analysis-ik 同样修复了该边界，且其算法主干与 u6 的等同性
//! 已由 testing/ab 的 C1 对拍证明（同词典 405 行 × 2 口径 34,072 token 仅
//! 3 个 token 级差异，全部可解释）。语料设计（无长单字重复串 / 无 astral /
//! 无量词门控触发字符 / 无大写词条）保证 B 的这些已知分歧不进入基线。
//!
//! **本测试锁定**：词元文本 + 类型 + 全局起止位置，逐 token 与 golden 一致。
//!
//! # 复现与重新生成
//!
//! ```bash
//! cd testing/parity && ./regen_goldens.sh   # 需要 JDK 21 + testing/ab 工装
//! ```

use ik_analyzer::{Config, Lexeme, TokenizerFactory};

/// 长文差分语料（8 行，4097–20000 字符；换页阈值 4096 的四长度点 + 五类跨页探针）。
const CORPUS: &str = include_str!("../testing/parity/corpus_long.txt");
/// smart 口径 golden（analysis-ik 输出，类型名已归一）。
const GOLDEN_SMART: &str = include_str!("../testing/parity/golden_b_smart.txt");
/// 细粒度（maxword）口径 golden。
const GOLDEN_MAXWORD: &str = include_str!("../testing/parity/golden_b_maxword.txt");

/// 词元类型名（拼写与 analysis-ik `Lexeme.getLexemeTypeString()` 一致，
/// 与 regen_goldens.sh 的归一规则对齐）。
fn type_name(t: i32) -> &'static str {
    match t {
        x if x == Lexeme::TYPE_ENGLISH => "ENGLISH",
        x if x == Lexeme::TYPE_ARABIC => "ARABIC",
        x if x == Lexeme::TYPE_LETTER => "LETTER",
        x if x == Lexeme::TYPE_CNWORD => "CN_WORD",
        x if x == Lexeme::TYPE_CNCHAR => "CN_CHAR",
        x if x == Lexeme::TYPE_OTHER_CJK => "OTHER_CJK",
        x if x == Lexeme::TYPE_CNUM => "CN_NUM",
        x if x == Lexeme::TYPE_COUNT => "COUNT",
        x if x == Lexeme::TYPE_CQUAN => "CN_QUAN",
        _ => "UNKNOWN",
    }
}

/// 富格式单行输出：`text[TYPE](begin-end)` 空格分隔（与 BProbe 输出同构）。
fn rich_line(factory: &TokenizerFactory, line: &str) -> String {
    factory
        .segmenter(line)
        .map(|l| {
            format!(
                "{}[{}]({}-{})",
                l.text(),
                type_name(l.lexeme_type()),
                l.begin_position(),
                l.end_position()
            )
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// 逐行逐 token 比对（含位置与类型），失败时报告首个分歧 token。
fn assert_matches(golden: &str, actual: &str, mode: &str) {
    let golden_lines: Vec<&str> = golden.lines().collect();
    let actual_lines: Vec<&str> = actual.lines().collect();
    assert_eq!(
        golden_lines.len(),
        actual_lines.len(),
        "{mode}: 行数不一致（golden {} vs actual {}）",
        golden_lines.len(),
        actual_lines.len()
    );
    for (i, (g, a)) in golden_lines.iter().zip(actual_lines.iter()).enumerate() {
        if g == a {
            continue;
        }
        let gt: Vec<&str> = g.split(' ').collect();
        let at: Vec<&str> = a.split(' ').collect();
        let j = (0..gt.len().min(at.len()))
            .find(|&k| gt[k] != at[k])
            .unwrap_or(gt.len().min(at.len()));
        let preview = CORPUS
            .lines()
            .nth(i)
            .map(|l| l.chars().take(12).collect::<String>())
            .unwrap_or_default();
        panic!(
            "{mode}: 第 {} 行与 golden 分歧（行输入 {preview:?}…）\n  首分歧 token #{j}\n  golden: {}\n  actual: {}",
            i + 1,
            gt.get(j).copied().unwrap_or("<EOF>"),
            at.get(j).copied().unwrap_or("<EOF>"),
        );
    }
}

/// smart 口径：8 行 × 4097–20000 字符，逐 token 对齐 analysis-ik golden。
#[test]
fn long_line_parity_goldens_smart() {
    let factory = TokenizerFactory::new(Config::u6_parity(true));
    let actual: String = CORPUS
        .lines()
        .map(|line| rich_line(&factory, line))
        .collect::<Vec<_>>()
        .join("\n");
    assert_matches(GOLDEN_SMART, &actual, "smart");
}

/// 细粒度口径：同上（maxword）。
#[test]
fn long_line_parity_goldens_maxword() {
    let factory = TokenizerFactory::new(Config::u6_parity(false));
    let actual: String = CORPUS
        .lines()
        .map(|line| rich_line(&factory, line))
        .collect::<Vec<_>>()
        .join("\n");
    assert_matches(GOLDEN_MAXWORD, &actual, "maxword");
}

/// 语料自身的完整性护栏：8 行、每行 > 4096（确保真的跨页）、纯 BMP、
/// 不含量词门控触发字符（两/几/数）——任一破坏都会使 golden 基线失效。
#[test]
fn long_line_corpus_integrity() {
    let lines: Vec<&str> = CORPUS.lines().collect();
    assert_eq!(lines.len(), 8, "语料应为 8 行");
    for (i, line) in lines.iter().enumerate() {
        assert!(
            line.chars().count() > 4096,
            "第 {} 行仅 {} 字符，未跨换页阈值",
            i + 1,
            line.chars().count()
        );
        assert!(
            line.chars().all(|c| (c as u32) <= 0xFFFF),
            "第 {} 行含 astral 字符（B 的 SurrogatePairSegmenter 会输出它，基线失效）",
            i + 1
        );
        assert!(
            !line.chars().any(|c| "两几数".contains(c)),
            "第 {} 行含量词门控触发字符（B 的 shouldMatchSingleChar 分歧，基线失效）",
            i + 1
        );
    }
}
