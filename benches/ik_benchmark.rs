// benches/ik_benchmark.rs

use criterion::{Criterion, Throughput, criterion_group, criterion_main};
use std::fs;
use std::hint::black_box;
use std::time::Duration;

// --- 导入新的公共 API ---
use ik_analyzer::{Config, TokenizerFactory};

// --- 配置 ---
const BENCH_TEXT_FILE: &str = "benches/corpus.txt";

/// 整合的性能基准测试
fn unified_benchmark(c: &mut Criterion) {
    // --- 准备阶段：一次性加载所有需要的数据 ---
    // corpus 缺失时直接 fail fast，避免静默回退到小文本导致基准结果不可比（评审 P4-2）。
    let text_to_segment = fs::read_to_string(BENCH_TEXT_FILE).unwrap_or_else(|e| {
        panic!("Benchmark corpus '{BENCH_TEXT_FILE}' not found: {e}. 请先准备语料库再运行基准。")
    });

    let text_bytes = text_to_segment.len();
    let chars: Vec<char> = text_to_segment.chars().collect();

    // =================================================================
    // 测试组 1: 端到端 (End-to-End) 分词性能
    // =================================================================
    {
        let mut group = c.benchmark_group("1. End-to-End Segmentation");
        group.measurement_time(Duration::from_secs(15));
        group.sample_size(30);
        group.throughput(Throughput::Bytes(text_bytes as u64));

        // 在测试组内部创建一次 Factory，其开销不计入 b.iter()
        let smart_factory = TokenizerFactory::new(Config::builder().use_smart(true).build());
        let fine_grained_factory =
            TokenizerFactory::new(Config::builder().use_smart(false).build());

        group.bench_function("Smart Mode", |b| {
            b.iter(|| {
                // 【核心修改】使用 factory 来创建轻量级的 segmenter
                let segmenter = smart_factory.segmenter(black_box(&text_to_segment));
                let count = segmenter.count();
                black_box(count);
            })
        });

        group.bench_function("Fine-Grained Mode", |b| {
            b.iter(|| {
                let segmenter = fine_grained_factory.segmenter(black_box(&text_to_segment));
                let count = segmenter.count();
                black_box(count);
            })
        });

        group.finish();
    }

    // =================================================================
    // 测试组 2: 核心 Match 操作性能
    // =================================================================
    {
        let factory_for_dict = TokenizerFactory::new(Config::builder().build());
        let dict = factory_for_dict.dictionary();

        let mut group = c.benchmark_group("2. Core Match Performance");
        group.measurement_time(Duration::from_secs(10));
        group.sample_size(50);

        let match_operations = chars.len().saturating_sub(5);
        group.throughput(Throughput::Elements(match_operations as u64));

        // 滑窗多字匹配。语义为**整段成词**（与 IK 原版 2012_u6 对齐，见
        // `compact_trie::match_text` 的文档）：只有整段走完且末节点成词才命中。
        // 因此此处统计的是"整段命中率"，与 Java 侧对比需按同一口径理解。

        group.bench_function("Sliding Window Match (size=5)", |b| {
            b.iter(|| {
                for i in 0..match_operations {
                    black_box(dict.match_in_main_dict(
                        black_box(&chars),
                        black_box(i),
                        black_box(5),
                    ));
                }
            })
        });

        group.finish();
    }
}

// 注册并运行我们统一的基准测试函数
criterion_group!(benches, unified_benchmark);
criterion_main!(benches);
