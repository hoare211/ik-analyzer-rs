//! 独立计时示例：对 smart / fine 两模式做端到端分词计时，并拆分
//! "构造 segmenter" 与 "纯分词" 的耗时，外加流式路径对照。
//!
//! 用法（release）：
//!   cargo run --release --example perf_cmp -- benches/corpus.txt

use ik_analyzer::{Config, TokenizerFactory};
use std::time::Instant;

fn main() {
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "benches/corpus.txt".to_string());
    let text = std::fs::read_to_string(&path).expect("read corpus");
    let n_chars = text.chars().count();

    let warmup = 20;
    let rounds = 50;

    println!("=== Rust ik-analyzer-rs (release) ===");
    println!("allocator: default (system)");
    println!("corpus: {path}");
    println!("chars: {n_chars}");
    println!("warmup={warmup} rounds={rounds}\n");

    // 词典构建时间（生产版只构建一次）
    let build_t = Instant::now();
    let smart_factory = TokenizerFactory::new(Config::builder().use_smart(true).build());
    let max_factory = TokenizerFactory::new(Config::builder().use_smart(false).build());
    let build_ms = build_t.elapsed().as_millis();
    println!("dict build (2 factories, once): {build_ms}ms\n");

    for (name, factory) in [("smart", &smart_factory), ("max  ", &max_factory)] {
        // warmup
        for _ in 0..warmup {
            let _ = factory.segmenter(&text).count();
        }
        let start = Instant::now();
        let mut total_tokens = 0usize;
        for _ in 0..rounds {
            total_tokens += factory.segmenter(&text).count();
        }
        let ms = start.elapsed().as_millis();
        let per_round = ms as f64 / rounds as f64;
        let tokens_per_run = total_tokens / rounds;
        println!(
            "[{name}] total={ms}ms  perRound={per_round:.2}ms  tokens/run={tokens_per_run}  throughput={:.0} chars/s  {:.0} tokens/s",
            n_chars as f64 / (per_round / 1000.0),
            tokens_per_run as f64 / (per_round / 1000.0)
        );
    }

    println!("\n--- Rust bottleneck breakdown (smart) ---");
    // A) 只构造 segmenter（含 StrCharSource 的 String 拷贝），不迭代
    for _ in 0..warmup {
        let _s = smart_factory.segmenter(&text);
    }
    let t0 = Instant::now();
    for _ in 0..rounds {
        let _s = smart_factory.segmenter(&text);
    }
    let construct_ms = t0.elapsed().as_millis();
    println!(
        "segmenter() construct only: {construct_ms}ms total, {:.2}ms/round",
        construct_ms as f64 / rounds as f64
    );

    // B) 构造 + 迭代
    for _ in 0..warmup {
        let _ = smart_factory.segmenter(&text).count();
    }
    let t1 = Instant::now();
    for _ in 0..rounds {
        let _ = smart_factory.segmenter(&text).count();
    }
    let full_ms = t1.elapsed().as_millis();
    println!(
        "segmenter() + consume   : {full_ms}ms total, {:.2}ms/round",
        full_ms as f64 / rounds as f64
    );
    println!(
        "=> pure tokenization (B-A): {:.2}ms/round",
        (full_ms - construct_ms) as f64 / rounds as f64
    );

    // C) 流式对照（复用 ReadCharSource 的缓冲）
    use std::io::Cursor;
    for _ in 0..warmup {
        let _ = smart_factory
            .segmenter_from_read(Cursor::new(text.clone().into_bytes()))
            .count();
    }
    let t2 = Instant::now();
    for _ in 0..rounds {
        let _ = smart_factory
            .segmenter_from_read(Cursor::new(text.clone().into_bytes()))
            .count();
    }
    let stream_ms = t2.elapsed().as_millis();
    println!(
        "segmenter_from_read (stream): {stream_ms}ms total, {:.2}ms/round",
        stream_ms as f64 / rounds as f64
    );
}
