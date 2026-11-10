# ik-analyzer-rs

**简体中文** | [English](README.en.md)

[![CI](https://github.com/hoare211/ik-analyzer-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/hoare211/ik-analyzer-rs/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/ik-analyzer-rs.svg)](https://crates.io/crates/ik-analyzer-rs)
[![docs.rs](https://docs.rs/ik-analyzer-rs/badge.svg)](https://docs.rs/ik-analyzer-rs)
[![PyPI](https://img.shields.io/pypi/v/ik-analyzer-py.svg)](https://pypi.org/project/ik-analyzer-py/)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

> **[EN]** A high-performance, thread-safe Rust Chinese word segmentation library from the
> [IK Analyzer](https://code.google.com/p/ik-analyzer/) lineage: built on the analysis-ik
> implementation baseline, with an explicit, opt-in IK Analyzer 2012_u6 parity contract.

**ik-analyzer-rs** 是一个**独立的 Rust 中文分词库**，算法血统承自 IK Analyzer，
提供 Rust / Python / CLI 三种入口：零 JVM 依赖、内存安全、线程安全、零 `unsafe`。

它**不是 IK Analyzer 2012_u6 的逐字移植**。项目按三层组织：

| 层 | 定位 |
|---|---|
| 实现层 | 以 **analysis-ik**（`master@6d2d70f`）为基准：吸收其相对 u6 的缺陷修复；其行为变更在本库实现为**可配置机制**，而非照搬 |
| 行为层（默认输出） | **产品决策，可以优于 u6**（默认装载停用词表、识别 CJK 扩展 B–I 汉字等）。不为了对齐牺牲出厂质量 |
| 兼容层（按需开启） | [`Config::u6_parity()`](docs/使用手册.md) 提供**显式、可差分验证**的 u6 行为契约（同有效配置 ⇒ 同输出），供需要与 u6 完全一致的下游使用 |

因此：**parity 是一项能力，不是本项目的定义**。当存在更优设计 —— 尤其是一份灵活配置
就能同时满足多种口径时 —— 本项目选择更优设计，不为对齐而对齐。

**正确性与可复现性**：仓库内 `cargo test` 共 81 个测试通过（含 13 组 MD5 级集成测试 +
405 行长文 parity golden × 2 口径）；开发期另以 [`testing/ab/`](testing/ab/README.md) 工装
完成 4×10⁶ 随机差分与 9133 行语料 × 4 口径的 Rust ↔ Java 对拍，0 分歧
（工装需自备 Java 侧源码）。

## ✨ 特性

- **独立实现、有所突破**：承袭 IK 的三子分词器 + 歧义裁决 + 双缓冲流式结构，但不照抄任何一版：吸收 analysis-ik 的缺陷修复、修掉 u6 的换页重叠/BOM/溢出问题，并把行为差异做成配置而非分支。
- **两种模式**：`smart`（歧义消解，粗粒度）/ `fine`（细粒度，高召回）。
- **生僻字全支持**：Rust `char` 为 Unicode 标量值，扩展支持 **CJK 扩展 B–I**（Unicode 15.1）；原版 Java `char` 会把 >U+FFFF 的补充平面字符丢弃。补充平面行为由 `AstralMode` 三值枚举显式控制（`Drop` / `Ideographs` / `Tokens`），一份配置同时满足严格 parity、产品默认与机制对齐三种需求。
- **可选的 u6 行为契约**：`Config::u6_parity(use_smart)` 一行切到冻结的 u6 口径（u6 分发词典 + 无停用词 + 大小写不敏感 + astral 丢弃），可差分验证；不开启时默认值是独立的产品决策。
- **配置便捷**：`ConfigBuilder` + `DictSource`（文件 / 内存字符串），告别原版 classpath 资源与二进制 `.dcm`。
- **高性能**：零 `unsafe`，子分词器热路径为静态分发（消除每字符 `dyn` 间接跳转）；歧义裁决为 O(n) 链模型（`judge-fast`）；提供 `segment_spans()` 零拷贝入口（实测分配次数 −98.6%）。
- **线程安全**：`TokenizerFactory` 加载词典一次，`Arc<Dictionary>` 多线程共享，同一进程可并存多部不同词典。
- **多语言生态**：Rust crate、`pyo3` Python 绑定（`ik-analyzer-py`）、内置 CLI。

## 🚀 快速开始

### Rust

```toml
[dependencies]
ik-analyzer-rs = "1.3.0"
```

```rust
use ik_analyzer::{Config, TokenizerFactory};

fn main() {
    // 1) 只建一次工厂（会加载并编译词典，开销较大）
    let factory = TokenizerFactory::new(Config::builder().use_smart(true).build());

    // 2) 每条文本用 segmenter() 产词元（极轻量）
    let words: Vec<String> = factory
        .segmenter("这是一个中文分词的例子")
        .map(|l| l.text().to_string())
        .collect();
    println!("{:?}", words); // ["这是", "一个", "中文", "分词", "例子"]
}
```

> ⚠️ **不要用已废弃的 `IKSegmenter::new(text, use_smart)`**：它每次都重建词典，开销巨大。始终走 `TokenizerFactory`。

### Python

```bash
pip install ik-analyzer-py
```

从源码构建（需要 `maturin`）：`cd python && maturin develop --release`。

```python
import ik_analyzer as ik

tok = ik.Tokenizer()                       # 默认 smart=True
print(tok.segment("欢迎使用本分词器"))     # ['欢迎', '使用', '本', '分词器']

cfg = ik.Config()
cfg.use_smart(False)                       # 细粒度
print(ik.Tokenizer(cfg).segment("张三说的确实在理"))
```

详见 [docs/使用手册.md](docs/使用手册.md)（含 `cut` 带位置、`segment_batch` 释放 GIL 批量处理、`Config` 全方法）。

### CLI

```bash
cargo run --bin ik-analyzer -- "研究生命起源"
cargo run --bin ik-analyzer -- "测试自定义词" --custom-dict custom/long_word_dict.dic
```

## 📊 性能

裁决由 O(n²) 优化为 O(n) 链模型（`judge-fast`，默认开启）。实测可复现：
`cargo run --release --example perf_cmp -- benches/corpus.txt`。

> ⚠️ **关于与原版的对比数字**：本仓库自测数据（`benches/corpus.txt`，288,712 字符 /
> 202,231 词元，release）可复现，例如 `cargo run --release --example alloc_count`。
> 但**未做同机同语料的 Java 性能对照测量** —— 历史文档中的 ~21ms / ~31ms 均为 2012 年
> 原版文档值（当时的机器与语料），无法在本机复现，仅可作**量级参考**，不构成同机对比结论。
> 故此处不给出"反超原版 X ms"之类的结论；性能对比请自行在同机同语料下测量。
>
> **正确性证据的可复现范围**：仓库内可直接复跑 `cargo test`（含 405 行长文 parity
> golden × 2 口径）；Java 侧 A/B 对拍工装位于 [`testing/ab/`](testing/ab/README.md)，
> 需自备 IK Analyzer 2012_u6 与 analysis-ik 源码树（均为 Apache-2.0）后按其 README
> 复算。开发期完成的 4×10⁶ 随机差分与 9133 行语料对拍同样由该工装产生，结论记为
> 0 分歧。

## 🏗️ 架构

```
TokenizerFactory (加载词典一次，共享 Arc<Dictionary>)
  └── IKSegmenter (每条文本一个，轻量，Iterator<Item=Lexeme>)
        ├── AnalyzeContext (双缓冲，流式)
        ├── CJKSegmenter / CnQuantifierSegmenter / LetterSegmenter
        └── IKArbitrator (歧义裁决)
```

## 📚 文档

- **[使用手册](docs/使用手册.md)** —— 安装、配置、两种模式、`Lexeme`、流式、错误、并发、Python 绑定、FAQ 等全场景。
- **[贡献指南](CONTRIBUTING.md)** —— 开发环境、测试矩阵、提交与 PR 规范。

## ⚠️ 重要提醒

- 使用 `TokenizerFactory`，勿用废弃的 `IKSegmenter::new`。
- 停用词会从最终输出中剔除（非单独成词元），与原版一致。
- **只有在显式启用 `Config::u6_parity(use_smart)` 时，才承诺与 u6 同配置同输出**。
  默认配置是独立的产品决策（smart + 大小写不敏感 + 全角→半角 + 内置停用词表 +
  `AstralMode::Ideographs`），与 u6 的差异全部可配置、默认关闭或已披露：
  - **u6 缺陷修复**（我们不同）：>4096 字符换页时 u6 每页边界重复输出 1 个重叠
    token；带 BOM 的词典 u6 污染首词条（我们剥离）；`sat_mul` 饱和（u6 `int`
    连乘可静默溢出，其词典下不可达）。
  - **表示层**：`offset` 单位为**码点**，与 Java 的 UTF-16 码元不可直接比较
    （Python 绑定为 byte 偏移）；词元文本/类型序列不受影响。
  - **可配置改进**：CJK 扩展 B–I 汉字默认识别（`AstralMode::Drop` 即 u6 口径）；
    emoji 等其余补充平面字符默认丢弃（`AstralMode::Tokens` 可输出）。
  - 默认装载内置停用词表（33 英文 = u6 分发 / analysis-ik 默认口径（与 u6 `stopword.dic` 逐字节相同）+ 69 中文 =
    本项目增强）；`Config::u6_parity` 置空之。
- 与原版的差异及改进，详见 [使用手册 §6.4](docs/使用手册.md#64-与-ik-2012_u6-的行为-parity-契约)。

## 兼容性 / MSRV

- 需要 **Rust 1.85+**（`edition = "2024"`）。`Cargo.toml` 已声明 `rust-version = "1.85"`。
- 默认启用 `judge-fast` feature（链模型 O(n) 裁决）。`--no-default-features` 回落到 `judge_reference`（O(n²)，与启用前行为一致，可供紧急回退或离线对拍）。
- CLI 日志后端为 optional：`cargo run --bin ik-analyzer --features env_logger -- "..."` 才会打印词典加载报告。

## 贡献

欢迎 Issue 与 PR。开始前请阅读 [CONTRIBUTING.md](CONTRIBUTING.md)；
安全问题请按 [SECURITY.md](SECURITY.md) 私下报告。

## License

[Apache-2.0](LICENSE)。算法与随包词表源自 IK Analyzer 家族：
[IK Analyzer 2012_u6](https://code.google.com/p/ik-analyzer/) 提供算法血统、词表与行为兼容基准，
[analysis-ik](https://github.com/infinilabs/analysis-ik) 是实现基准（两者均为 Apache-2.0）。
完整的第三方归属声明见 [NOTICE](NOTICE)。
