//! 测量运行期 DAWG 构建成本（方案 C'，验收数据来源）：
//!   cold -> 进程内首次构建（内嵌文本 → DawgBuilder → CompactTrie，三词典全量）
//!   warm -> 命中全局缓存的第二次构建（纯默认词表，应 ~µs 级）
//!
//! 用法（预算口径为 release）：
//!   cargo run --release --example measure_load -- cold
//!   cargo run --release --example measure_load -- warm
//! 内存用 /usr/bin/time -v 在外部采集（Maximum resident set size，
//! 构建期峰值 RSS 的观测点在 cold 模式）。

use ik_analyzer::Dictionary;
use ik_analyzer::cfg::Config;
use std::time::Instant;

/// 当前进程 RSS（KB）。statm 第二字段为常驻页数，乘页大小。
fn current_rss_kb() -> Option<u64> {
    let statm = std::fs::read_to_string("/proc/self/statm").ok()?;
    let resident_pages: u64 = statm.split_whitespace().nth(1)?.parse().ok()?;
    Some(resident_pages * 4096 / 1024)
}

fn main() {
    let mode = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "cold".to_string());

    // warm 模式先构建一次填充全局缓存
    if mode == "warm" {
        let cfg = Config::builder().build();
        let _ = Dictionary::try_new(&cfg).expect("warm-up build failed");
    }

    let cfg = Config::builder().build();
    let t0 = Instant::now();
    let dict = Dictionary::try_new(&cfg).expect("build failed");
    let elapsed = t0.elapsed();

    // 确认词典确实可用（避免被优化掉）
    let sample = dict
        .match_in_main_dict(&"中华人民共和国".chars().collect::<Vec<_>>(), 0, 7)
        .is_match();

    println!(
        "[{}] build={:.3?} | default_dict_loaded=true | main_has_中华人民共和国={} | rss_after_build={}KB",
        mode,
        elapsed,
        sample,
        current_rss_kb().unwrap_or(0)
    );

    // 给 200ms 稳态，便于 /usr/bin/time -v 抓到稳定 RSS
    std::thread::sleep(std::time::Duration::from_millis(200));
}
