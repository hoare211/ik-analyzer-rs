use ik_analyzer::Dictionary;
use ik_analyzer::{Config, TokenizerFactory};
use std::time::Instant;

fn main() {
    let dict = Dictionary::new(&Config::builder().build());
    let text = "中华人民共和国人民大会堂".repeat(24_000 / 14 + 1);
    let chars: Vec<char> = text.chars().collect();
    let n = chars.len();

    // 模拟 CJKSegmenter 的逐字最长匹配：对每个 begin 做一次 match_in_main_dict 单步
    let warmup = 20;
    let rounds = 50;

    // (1) 单步查找：每个字符做 match_in_main_dict(chars, i, 1)
    for _ in 0..warmup {
        for i in 0..n {
            std::hint::black_box(dict.match_in_main_dict(&chars, i, 1));
        }
    }
    let t = Instant::now();
    let mut cnt = 0u64;
    for _ in 0..rounds {
        for i in 0..n {
            let _ = dict.match_in_main_dict(&chars, i, 1);
            cnt += 1;
        }
    }
    let ms = t.elapsed().as_millis();
    println!(
        "[Trie 单步] {} 次查找 = {}ms, 单次 = {:.3}µs, 全文本 {} 字符单步仅 Trie 已占 {}ms/round",
        cnt / rounds as u64,
        ms,
        (ms as f64 / (n as f64 * rounds as f64) * 1000.0),
        n,
        ms as f64 / rounds as f64
    );

    // (2) 对照：完整分词（smart）单轮耗时，看 Trie 单步占比
    let factory = TokenizerFactory::new(Config::builder().use_smart(true).build());
    for _ in 0..warmup {
        let _ = factory.segmenter(&text).count();
    }
    let t2 = Instant::now();
    let mut tot = 0usize;
    for _ in 0..rounds {
        tot += factory.segmenter(&text).count();
    }
    let ms2 = t2.elapsed().as_millis();
    println!(
        "[完整分词 smart] {}ms/round, tokens/run={}",
        ms2 as f64 / rounds as f64,
        tot / rounds
    );
    println!(
        "=> Trie 单步查找占完整分词比例 ≈ {:.1}%",
        (ms as f64 / rounds as f64) / (ms2 as f64 / rounds as f64) * 100.0
    );
}
