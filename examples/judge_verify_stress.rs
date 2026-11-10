//! 阶段 2 加压轮（维度扩展）：在 `judge-fast` + `judge-verify` 同开时，对真实分词路径做
//! 三个维度的差分加压。本 example 自身不实现差分逻辑——`judge-verify` 已在 `IKArbitrator::judge`
//! 内部对**每一个真实 cross_path** 跑 ref/fast 双实现并逐位 assert，任何分歧会直接 panic。
//! 本 example 负责把"生产路径"推进到各种极端形态，并附加两条硬断言：
//!
//! 1. 流式不变性：同一语料用不同字节分块大小（含 1 字节极端分块）分词，token 指纹必须完全一致。
//!    （`ReadCharSource` 跨 read 边界拼接 UTF-8，分块不应改变结果——这是流式路径的正确性不变量。）
//! 2. 并发确定性：N 个线程各跑完全相同的全量任务，每个 (combo, corpus, chunk) 的指纹必须跨线程全等。
//!    （`IKArbitrator` 无字段、各 `IKSegmenter` 实例独立，本加压确认无隐藏全局可变状态/数据竞争。）
//!
//! 端到端一致性另由"两次二进制 diff"交叉验证：不带 `judge-fast`（纯 ref）与带 `judge-fast
//! judge-verify`（fast + 内部 assert）跑同一命令，输出指纹应逐行一致。
//!
//! 用法：
//!   judge_verify_stress [corpus_path ...] [--chunks "0,1,3,7,64,4096"] [--threads N]
//!                       [--dict "词\n词"] [--stop "字\n字"] [--quant "量\n量"]
//!   - 位置参数：语料文件（多可多个）；不存在则当作内联文本。
//!   - --chunks：模拟 `segmenter_from_read` 的字节分块大小；0 = 整段 `StrCharSource`（不分块）。
//!   - 内置 5 种 config 组合（base / +dict / +stop / +quant / +all）自动覆盖自定义词典维度。
//!
//! 指纹格式（每行）：`{combo}\t{corpus}\t{chunk}\t{ntok}\t{fnv1a:016x}`

use std::collections::BTreeMap;
use std::io::{self, Cursor, Read};
use std::path::Path;
use std::process;
use std::sync::Arc;

// 聚合结果的类型别名（评审 P2 type_complexity）。
type FingerprintMap = BTreeMap<(String, String, usize), (usize, u64)>;
use std::thread;

use ik_analyzer::{Config, TokenizerFactory};

/// 模拟任意字节分块边界：每次 `read` 最多返回 `limit` 字节，迫使 `ReadCharSource` 拼接跨边界 UTF-8。
struct ChunkedReader<R: Read> {
    inner: R,
    limit: usize,
}
impl<R: Read> Read for ChunkedReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return self.inner.read(buf);
        }
        let take = self.limit.max(1).min(buf.len());
        self.inner.read(&mut buf[..take])
    }
}

/// FNV-1a 64，确定性、无外部依赖，用于把 token 序列压成稳定指纹。
fn fnv1a_byte(h: u64, b: u8) -> u64 {
    let h = h ^ (b as u64);
    h.wrapping_mul(0x100000001b3)
}

/// 对一段语料（chunk 指定分块大小，0 = 整段 `StrCharSource`）跑分词，返回 (token 数, FNV 指纹)。
/// 注意：`judge-verify` 模式下，本函数内部的 `segmenter.next()` 会触发 `judge()` 的逐 cross_path
/// 双跑 assert；一旦 fast 与 ref 分歧，此处直接 panic。
fn run_one(factory: &TokenizerFactory, text: &str, chunk: usize) -> (usize, u64) {
    // 统一字节预算：所有 chunk 大小必须处理**完全相同**的逻辑输入，流式不变性才有意义。
    // 截断点对 chunk=0 的 `&str` 与 chunk>=1 的字节序列必须一致——故向下对齐到字符边界，
    // 保证两种路径喂入同一段合法 UTF-8。
    const BUDGET: usize = 500_000;
    let raw = text.as_bytes();
    let mut cut = raw.len().min(BUDGET);
    while cut > 0 && !text.is_char_boundary(cut) {
        cut -= 1;
    }
    let text_slice = &text[..cut];

    let seg = if chunk == 0 {
        factory.segmenter(text_slice)
    } else {
        let bytes = raw[..cut].to_vec();
        factory.segmenter_from_read(ChunkedReader {
            inner: Cursor::new(bytes),
            limit: chunk,
        })
    };

    let mut h = 0xcbf29ce484222325u64;
    let mut ntok = 0usize;
    for l in seg {
        ntok += 1;
        for &b in l.text().as_bytes() {
            h = fnv1a_byte(h, b);
        }
        for &x in &(l.begin_position() as u64).to_le_bytes() {
            h = fnv1a_byte(h, x);
        }
        for &x in &(l.end_position() as u64).to_le_bytes() {
            h = fnv1a_byte(h, x);
        }
    }
    (ntok, h)
}

// 自定义词典维度用的内置词/停用词/量词（合法即可，不要求出现在语料中）。
const DICT_EXTRA: &str = "深度学习模型\n分词器优化\n自然语言处理\n长春市长春药店\n结合成分子时\n";
const STOP_EXTRA: &str = "的\n了\n和\n是\n在\n与\n及\n";
const QUANT_EXTRA: &str = "颗\n尾\n尊\n匹\n";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut corpora: Vec<String> = Vec::new();
    let mut threads = 1usize;
    let mut chunks = vec![0usize];

    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--threads" => {
                threads = args[i + 1].parse().expect("threads must be usize");
                i += 2;
            }
            "--chunks" => {
                chunks = args[i + 1]
                    .split(',')
                    .map(|s| s.trim().parse().expect("chunk must be usize"))
                    .collect();
                i += 2;
            }
            other => {
                corpora.push(other.to_string());
                i += 1;
            }
        }
    }

    // 内置歧义语料（保证触发 judge），始终覆盖；再追加用户文件。
    let mut texts: Vec<(String, String)> = vec![
        (
            "inline:长春".to_string(),
            "长春市长春药店的长春花开了，长春的春天真美".to_string(),
        ),
        (
            "inline:结合".to_string(),
            "结合成分子时，应当结合实际进行综合分析研究".to_string(),
        ),
        (
            "inline:人民银行".to_string(),
            "中国人民银行是中国人民银行，人民币国际化稳步推进".to_string(),
        ),
        (
            "inline:中华".to_string(),
            "中华人民共和国国歌是中华民族的最强音".to_string(),
        ),
        (
            "inline:混排".to_string(),
            "第1个iPhone15Pro手机售价8999.5元约1.2万人民币AT&T2024年3月5日".to_string(),
        ),
    ];
    for c in &corpora {
        if let Ok(content) = std::fs::read_to_string(c) {
            let name = Path::new(c)
                .file_name()
                .map(|f| f.to_string_lossy().into_owned())
                .unwrap_or_else(|| c.clone());
            texts.push((name, content));
        } else {
            texts.push((format!("inline:{c}"), c.clone()));
        }
    }

    // 5 种 config 组合（自定义词典维度）。
    let combos: Vec<(String, TokenizerFactory)> = vec![
        (
            "base".to_string(),
            TokenizerFactory::new(Config::builder().use_smart(true).build()),
        ),
        (
            "+dict".to_string(),
            TokenizerFactory::new(
                Config::builder()
                    .use_smart(true)
                    .add_ext_dict_owned_text(DICT_EXTRA.to_string())
                    .build(),
            ),
        ),
        (
            "+stop".to_string(),
            TokenizerFactory::new(
                Config::builder()
                    .use_smart(true)
                    .add_ext_stop_word_owned_text(STOP_EXTRA.to_string())
                    .build(),
            ),
        ),
        (
            "+quant".to_string(),
            TokenizerFactory::new(
                Config::builder()
                    .use_smart(true)
                    .add_ext_quantifier_owned_text(QUANT_EXTRA.to_string())
                    .build(),
            ),
        ),
        (
            "+all".to_string(),
            TokenizerFactory::new(
                Config::builder()
                    .use_smart(true)
                    .add_ext_dict_owned_text(DICT_EXTRA.to_string())
                    .add_ext_stop_word_owned_text(STOP_EXTRA.to_string())
                    .add_ext_quantifier_owned_text(QUANT_EXTRA.to_string())
                    .build(),
            ),
        ),
    ];

    let texts = Arc::new(texts);
    let combos = Arc::new(combos);
    let chunks = chunks.clone();

    // 多线程框架统一处理（threads==1 即退化为单线程，一致性检查平凡通过）。
    let mut handles = Vec::new();
    for _ in 0..threads.max(1) {
        let texts = Arc::clone(&texts);
        let combos = Arc::clone(&combos);
        let chunks = chunks.clone();
        let handle = thread::spawn(move || {
            let mut map: BTreeMap<(String, String, usize), (usize, u64)> = BTreeMap::new();
            for ci in 0..combos.len() {
                for (name, text) in texts.iter() {
                    for &k in &chunks {
                        let (n, h) = run_one(&combos[ci].1, text, k);
                        map.insert((combos[ci].0.clone(), name.clone(), k), (n, h));
                    }
                }
            }
            map
        });
        handles.push(handle);
    }

    let mut results: Vec<FingerprintMap> = Vec::new();
    for h in handles {
        match h.join() {
            Ok(m) => results.push(m),
            Err(_) => {
                eprintln!(
                    "FATAL: 某线程 panic（极可能是 judge-verify 内部 assert 触发 fast≠ref 分歧）"
                );
                process::exit(2);
            }
        }
    }

    // 硬断言 2：并发确定性。每个 (combo, corpus, chunk) 跨线程指纹必须全等。
    for ci in 0..combos.len() {
        for (name, _t) in texts.iter() {
            for &k in &chunks {
                let key = (combos[ci].0.clone(), name.clone(), k);
                let first = results[0].get(&key).copied();
                if results.iter().any(|m| m.get(&key) != first.as_ref()) {
                    eprintln!(
                        "NON-DETERMINISTIC: combo={} corpus={} chunk={} 跨线程指纹不一致",
                        combos[ci].0, name, k
                    );
                    process::exit(1);
                }
            }
        }
    }

    // 硬断言 1：流式不变性。同一 (combo, corpus) 下，所有 chunk 大小的分词指纹必须一致。
    for ci in 0..combos.len() {
        for (name, _t) in texts.iter() {
            let mut hs: Vec<u64> = Vec::new();
            for &k in &chunks {
                hs.push(
                    results[0]
                        .get(&(combos[ci].0.clone(), name.clone(), k))
                        .copied()
                        .unwrap()
                        .1,
                );
            }
            if hs.iter().any(|&x| x != hs[0]) {
                eprintln!(
                    "STREAM-INVARIANT VIOLATED: combo={} corpus={} 不同分块大小产生不同分词结果",
                    combos[ci].0, name
                );
                for (&k, &h) in chunks.iter().zip(hs.iter()) {
                    let n = results[0]
                        .get(&(combos[ci].0.clone(), name.clone(), k))
                        .copied()
                        .unwrap()
                        .0;
                    eprintln!("  chunk={:>5} ntok={:>7} hash={:016x}", k, n, h);
                }
                process::exit(3);
            }
        }
    }

    // 输出指纹（供两次二进制 diff）。
    for (name, _t) in texts.iter() {
        for ci in 0..combos.len() {
            for &k in &chunks {
                let (n, h) = results[0]
                    .get(&(combos[ci].0.clone(), name.clone(), k))
                    .copied()
                    .unwrap();
                println!("{}\t{}\t{}\t{}\t{:016x}", combos[ci].0, name, k, n, h);
            }
        }
    }
    eprintln!(
        "OK: {} combos × {} corpora × {} chunks on {} threads, all assertions passed",
        combos.len(),
        texts.len(),
        chunks.len(),
        threads.max(1)
    );
}
