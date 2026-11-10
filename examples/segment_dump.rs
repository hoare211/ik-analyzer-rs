//! 逐行分词对账工具：stdin 读入文本，stdout 每行输出 `/` 连接的词元。
//!
//! 用途：与其它实现（如 tag `v1.0.0`、Java 原版 `IKSegmenter`）做**逐行差分对账**——
//! 两侧喂同一份语料，`diff` 输出即可定位分歧行。配合 `--no-stop-words` 可复现
//! v1.0.0 的语义（v1.0.0 的 `dict/ext_stop_word/stop.dic` 是 0 字节，不过滤停用词）。
//!
//! ```bash
//! # 当前默认（= v1.0.0 语义：smart + 大小写不敏感），但保留停用词过滤
//! cat corpus.txt | cargo run --release --example segment_dump
//!
//! # 严格复现 v1.0.0（含停用词表为空）
//! cat corpus.txt | cargo run --release --example segment_dump -- --no-stop-words
//!
//! # IK 2012_u6 行为 parity 预设（u6 分发词典 + 无停用词 + 大小写不敏感 + astral 丢弃）
//! cat corpus.txt | cargo run --release --example segment_dump -- --u6-parity
//!
//! # 细粒度模式 / 保留原文大小写
//! cat corpus.txt | cargo run --release --example segment_dump -- --smart false
//! cat corpus.txt | cargo run --release --example segment_dump -- --lowercase false
//! ```
use ik_analyzer::{AstralMode, Config, TokenizerFactory};
use std::io::{self, Read, Write};

fn flag(args: &[String], key: &str, default: bool) -> bool {
    args.iter()
        .position(|a| a == key)
        .and_then(|i| args.get(i + 1))
        .map(|v| v == "true")
        .unwrap_or(default)
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let smart = flag(&args, "--smart", true);
    let lowercase = flag(&args, "--lowercase", true);
    let clear_stop_words = args.iter().any(|a| a == "--no-stop-words");
    // IK 2012_u6 行为 parity 预设（设计决策 1）：
    // 与 Java u6 可执行体做差分对账时用此口径（--u6-parity 优先于其余旗标）。
    let u6_parity = args.iter().any(|a| a == "--u6-parity");
    // 追加扩展词典（对账补丁用，如两侧同补「第三十」——见
    // 缺陷分析记录的对账装配）
    let ext_dicts: Vec<std::path::PathBuf> = args
        .iter()
        .position(|a| a == "--ext-dict")
        .and_then(|i| args.get(i + 1))
        .map(|p| vec![std::path::PathBuf::from(p)])
        .unwrap_or_default();

    let factory = if u6_parity {
        // u6 parity 口径：u6 分发词典 + 停用词置空 + 大小写不敏感 + astral 丢弃。
        // 无补丁时直接用冻结预设；有 ext 补丁时以等价 builder 装配
        // （`Config::u6_parity` 是冻结契约，不承载补丁）。
        if ext_dicts.is_empty() {
            Config::u6_parity(smart)
        } else {
            Config::builder()
                .use_smart(smart)
                .enable_lowercase(true)
                .astral_mode(AstralMode::Drop)
                .with_stop_words(Vec::new())
                .add_ext_dict_paths(ext_dicts)
                .build()
        }
    } else {
        let mut builder = Config::builder()
            .use_smart(smart)
            .enable_lowercase(lowercase);
        if clear_stop_words {
            builder = builder.with_stop_words(Vec::new());
        }
        if !ext_dicts.is_empty() {
            builder = builder.add_ext_dict_paths(ext_dicts);
        }
        builder.build()
    };
    let factory = TokenizerFactory::new(factory);

    let mut input = String::new();
    io::stdin().read_to_string(&mut input).unwrap();

    let stdout = io::stdout();
    let mut out = stdout.lock();
    // 富格式（与 testing/ab 的 AProbe/BProbe 对齐）：text[TYPE](begin-end) 空格分隔，
    // 类型名用 B 侧 getLexemeTypeString() 的拼写（CN_WORD/CN_CHAR/…）。
    let rich = args.iter().any(|a| a == "--rich");
    for line in input.lines() {
        if rich {
            let parts: Vec<String> = factory
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
                .collect();
            writeln!(out, "{}", parts.join(" ")).unwrap();
        } else {
            let words: Vec<String> = factory
                .segmenter(line)
                .map(|l| l.text().to_string())
                .collect();
            writeln!(out, "{}", words.join("/")).unwrap();
        }
    }
}

/// 词元类型名（拼写与 analysis-ik `Lexeme.getLexemeTypeString()` 一致，
/// 便于与 testing/ab 的 AProbe/BProbe 输出直接 diff）。
fn type_name(t: i32) -> &'static str {
    match t {
        x if x == ik_analyzer::Lexeme::TYPE_ENGLISH => "ENGLISH",
        x if x == ik_analyzer::Lexeme::TYPE_ARABIC => "ARABIC",
        x if x == ik_analyzer::Lexeme::TYPE_LETTER => "LETTER",
        x if x == ik_analyzer::Lexeme::TYPE_CNWORD => "CN_WORD",
        x if x == ik_analyzer::Lexeme::TYPE_CNCHAR => "CN_CHAR",
        x if x == ik_analyzer::Lexeme::TYPE_OTHER_CJK => "OTHER_CJK",
        x if x == ik_analyzer::Lexeme::TYPE_CNUM => "CN_NUM",
        x if x == ik_analyzer::Lexeme::TYPE_COUNT => "COUNT",
        x if x == ik_analyzer::Lexeme::TYPE_CQUAN => "CN_QUAN",
        _ => "UNKNOWN",
    }
}
