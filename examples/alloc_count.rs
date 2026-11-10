//! 分词热路径的堆分配计量工具（评审 P3-2 / P2-1 / P2-2）。
//!
//! # 为什么需要它
//!
//! 20260930 的P1-8 删除了 `probe-count` / `probe-mimalloc` 两个 feature，但
//! **没有替代物**，导致"分配量"这一维度此后无法量化 —— 而它恰恰是本项目
//! 性能叙事的关键（"零unsafe / 零分配热路径"）。2026-10-07 评审量化 P2-1/P2-2
//! 时不得不临时手写一份计数分配器。
//!
//! 本 example 即那个工具的常驻版本：`#[global_allocator]` 放在 example 内部，
//! **不需要任何 feature gate**，因而既不污染下游 crate 的编译，也不占用发布产物的
//! feature 语义。手册中原先指向 `probe-count` 的命令应改指向本文件。
//!
//! # 用法
//!
//! ```bash
//! # 默认：跑 benches/corpus.txt，输出 耗时 / 分配次数 / 分配字节 / 每字符指标
//! cargo run --release --example alloc_count
//!
//! # 对照 judge_reference 回落路径
//! cargo run --release --no-default-features --example alloc_count
//!
//! # 指定语料
//! cargo run --release --example alloc_count -- path/to/corpus.txt
//! ```
//!
//! # 解读
//!
//! `allocs_per_char` 是关键指标：分词是逐字符线性过程，故该值应与输入长度无关
//! （稳定在某个常数）。若随语料长度线性增长，说明存在按字符的堆分配。
//!
//! `bytes_per_char` 中占比最大的一项通常是**每输出词元一个 `String`**
//! （`analyze_context::get_next_lexeme` 的惰性文本生成，占实测分配次数的 94%）。
//! 零拷贝入口 `IKSegmenter` 目前尚未提供，见评审 P2-1。

use ik_analyzer::{Config, TokenizerFactory};
use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

/// 计数包装的全局分配器：只统计 `alloc`/`realloc` 的次数与字节数，
/// `dealloc` 不计数（本工具关心的是"分配压力"而非存活内存）。
struct Counting;

static ALLOC_COUNT: AtomicU64 = AtomicU64::new(0);
static ALLOC_BYTES: AtomicU64 = AtomicU64::new(0);

unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOC_COUNT.fetch_add(1, Ordering::Relaxed);
        ALLOC_BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        ALLOC_COUNT.fetch_add(1, Ordering::Relaxed);
        ALLOC_BYTES.fetch_add(new_size as u64, Ordering::Relaxed);
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

fn default_corpus() -> String {
    std::fs::read_to_string("benches/corpus.txt").unwrap_or_else(|e| {
        panic!("默认语料 benches/corpus.txt 不可读（{e}）。请显式传入语料路径：\n  cargo run --release --example alloc_count -- <corpus.txt>")
    })
}

fn main() {
    let arg = std::env::args().nth(1);
    let corpus = arg.map_or_else(default_corpus, |p| {
        std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("语料 {p} 不可读：{e}"))
    });
    let n_chars = corpus.chars().count();

    let judge = if cfg!(feature = "judge-fast") {
        "judge_fast (默认生产路径)"
    } else {
        "judge_reference (--no-default-features 回落路径)"
    };
    println!("corpus      : {n_chars} chars");
    println!("judge       : {judge}");

    // 预热：词典构建等一次性开销不计入测量
    let factory = TokenizerFactory::new(Config::builder().use_smart(true).build());
    black_box(factory.segmenter(&corpus).count());

    let allocs_before = ALLOC_COUNT.load(Ordering::Relaxed);
    let bytes_before = ALLOC_BYTES.load(Ordering::Relaxed);
    let start = Instant::now();
    let tokens = factory.segmenter(black_box(&corpus)).count();
    let elapsed = start.elapsed();
    let allocs = ALLOC_COUNT.load(Ordering::Relaxed) - allocs_before;
    let bytes = ALLOC_BYTES.load(Ordering::Relaxed) - bytes_before;

    let mib = |b: u64| b as f64 / (1024.0 * 1024.0);
    println!("tokens      : {tokens}");
    println!("elapsed     : {elapsed:?}");
    println!(
        "allocs      : {allocs} ({:.3}/char)",
        allocs as f64 / n_chars as f64
    );
    println!(
        "alloc bytes : {bytes} ({:.2} MiB, {:.1}/char)",
        mib(bytes),
        bytes as f64 / n_chars as f64
    );
    println!("allocs/token: {:.2}", allocs as f64 / tokens.max(1) as f64);

    // ---- 对照：零拷贝路径 segment_spans（不生成词元文本）----
    let allocs_before = ALLOC_COUNT.load(Ordering::Relaxed);
    let bytes_before = ALLOC_BYTES.load(Ordering::Relaxed);
    let start = Instant::now();
    let span_tokens = factory.segment_spans(black_box(&corpus)).count();
    let elapsed = start.elapsed();
    let span_allocs = ALLOC_COUNT.load(Ordering::Relaxed) - allocs_before;
    let span_bytes = ALLOC_BYTES.load(Ordering::Relaxed) - bytes_before;

    println!();
    println!("--- segment_spans（零拷贝，对照）---");
    println!("tokens      : {span_tokens}");
    println!("elapsed     : {elapsed:?}");
    println!(
        "allocs      : {span_allocs} ({:.3}/char)",
        span_allocs as f64 / n_chars as f64
    );
    println!(
        "alloc bytes : {span_bytes} ({:.2} MiB, {:.1}/char)",
        mib(span_bytes),
        span_bytes as f64 / n_chars as f64
    );
    println!(
        "对比        : 分配次数 -{:.1}%，分配字节 -{:.1}%",
        100.0 * (1.0 - span_allocs as f64 / allocs.max(1) as f64),
        100.0 * (1.0 - span_bytes as f64 / bytes.max(1) as f64)
    );
}
