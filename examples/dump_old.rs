//! 【对账工具·2025-09 ~ 2026-08 年代的线性版本专用】
//!
//! 与旧版仓库的 `tools/dump_v1.rs` 同构（该文件不在本仓库），沿用 `with_stop_words` +
//! `add_ext_dict_text` / `add_ext_dict_paths(Vec<PathBuf>)`，并把词典口径对齐到 **Java 原版 2012_u6**：
//! - 清空停用词（Java 侧我们已把 `IKAnalyzer.cfg.xml` 的 ext_stopwords 置空）；
//! - 补上 Java `main2012.dic` 独有、而我们 `dict/main2012.dic` 缺失的那 1 个词 `第三十`。
//!
//! 用法：`dump_old [smart|fine] [--ext-dict <path>]`
//! （stdin 逐行 → stdout 逐行 `/` 连接词元；`===READY===` 之后为有效输出）
use ik_analyzer::{Config, TokenizerFactory};
use std::io::{self, Read, Write};
use std::path::PathBuf;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let smart = !args.iter().any(|a| a == "fine");

    println!("===BEGIN===");
    let mut builder = Config::builder()
        .use_smart(smart)
        .with_stop_words(Vec::new())
        .add_ext_dict_text("第三十");
    if let Some(i) = args.iter().position(|a| a == "--ext-dict") {
        match args.get(i + 1) {
            Some(p) => builder = builder.add_ext_dict_paths(vec![PathBuf::from(p)]),
            None => {
                eprintln!("--ext-dict requires a path");
                return;
            }
        }
    }
    let factory = TokenizerFactory::new(builder.build());
    println!("===READY===");

    let mut input = String::new();
    io::stdin().read_to_string(&mut input).unwrap();
    let stdout = io::stdout();
    let mut out = stdout.lock();
    for line in input.lines() {
        let words: Vec<String> = factory
            .segmenter(line)
            .map(|l| l.text().to_string())
            .collect();
        writeln!(out, "{}", words.join("/")).unwrap();
    }
}
