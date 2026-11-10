# ik-analyzer-rs

[![CI](https://github.com/hoare211/ik-analyzer-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/hoare211/ik-analyzer-rs/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/ik-analyzer-rs.svg)](https://crates.io/crates/ik-analyzer-rs)
[![docs.rs](https://docs.rs/ik-analyzer-rs/badge.svg)](https://docs.rs/ik-analyzer-rs)
[![PyPI](https://img.shields.io/pypi/v/ik-analyzer-py.svg)](https://pypi.org/project/ik-analyzer-py/)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](https://github.com/hoare211/ik-analyzer-rs/blob/main/LICENSE)

**English** | [简体中文](https://github.com/hoare211/ik-analyzer-rs/blob/main/README.md)

A high-performance, thread-safe Rust Chinese word segmentation library from the
[IK Analyzer](https://code.google.com/p/ik-analyzer/) lineage: built on the analysis-ik
implementation baseline, with an explicit, opt-in IK Analyzer 2012_u6 parity contract.
It ships Rust, Python and CLI entry points, with no JVM dependency, memory safety,
thread safety and zero `unsafe`.

It is **not** a line-by-line port of IK Analyzer 2012_u6. The project is organized in three layers:

| Layer | Positioning |
|---|---|
| Implementation | Based on **analysis-ik** (`master@6d2d70f`): its defect fixes relative to u6 are absorbed, and its behavioral changes are exposed as **configurable mechanisms** rather than copied verbatim |
| Behavior (default output) | **Product decisions that may improve on u6** (bundled stop-word list, CJK Extension B–I ideographs, ...). We do not sacrifice production quality for parity |
| Compatibility (opt-in) | [`Config::u6_parity()`](https://github.com/hoare211/ik-analyzer-rs/blob/main/docs/USAGE.en.md) provides an **explicit, differentially verifiable** u6 behavior contract (same effective configuration ⇒ same output) for downstream users who need exact u6 compatibility |

In short: **parity is a capability, not the definition of this project**. When a better
design exists — especially when one flexible configuration can satisfy several regimes at
once — this project chooses the better design rather than aligning for alignment's sake.

**Correctness and reproducibility**: `cargo test` in this repository covers 81 tests
(including 13 MD5-level integration tests and a 405-line long-text parity golden set × 2
modes). During development, the [`testing/ab/`](https://github.com/hoare211/ik-analyzer-rs/blob/main/testing/ab/README.md) harness was used for
4×10⁶ randomized differential runs and a 9133-line corpus × 4 configurations Rust ↔ Java
comparison with 0 divergences (the harness requires your own Java-side sources).

## ✨ Features

- **Independent implementation, with deliberate improvements**: inherits IK's three sub-segmenters, ambiguity arbitration and double-buffered streaming structure, but copies no single version: it absorbs analysis-ik's defect fixes, fixes u6's page-boundary duplication/BOM/overflow problems, and turns behavioral differences into configuration instead of branches.
- **Two modes**: `smart` (ambiguity resolution, coarse-grained) / `fine` (fine-grained, high recall).
- **Full rare-character support**: Rust `char` is a Unicode scalar value, so **CJK Extension B–I** (Unicode 15.1) is supported; the original Java `char` drops supplementary-plane characters above U+FFFF. Astral behavior is controlled by the three-value `AstralMode` enum (`Drop` / `Ideographs` / `Tokens`), where one configuration satisfies strict parity, product defaults and mechanism alignment at once.
- **Optional u6 behavior contract**: `Config::u6_parity(use_smart)` switches to a frozen u6 regime (u6 distribution dictionaries + no stop words + case-insensitive + astral dropped), verifiable by differential tests; when not enabled, defaults are independent product decisions.
- **Convenient configuration**: `ConfigBuilder` + `DictSource` (file / in-memory string), replacing classpath resources and binary `.dcm` files.
- **High performance**: zero `unsafe`; sub-segmenters are statically dispatched on the hot path (no per-character `dyn` indirection); arbitration uses an O(n) chain model (`judge-fast`); `segment_spans()` provides a zero-copy entry point (measured −98.6% allocations).
- **Thread safety**: `TokenizerFactory` loads dictionaries once and shares `Arc<Dictionary>` across threads; multiple different dictionaries can coexist in one process.
- **Multi-language ecosystem**: Rust crate, `pyo3` Python bindings (`ik-analyzer-py`) and a built-in CLI.

## 🚀 Quick start

### Rust

```toml
[dependencies]
ik-analyzer-rs = "1.3.0"
```

```rust
use ik_analyzer::{Config, TokenizerFactory};

fn main() {
    // 1) Build the factory once (loads and compiles dictionaries; expensive)
    let factory = TokenizerFactory::new(Config::builder().use_smart(true).build());

    // 2) Create a segmenter per text (very cheap)
    let words: Vec<String> = factory
        .segmenter("这是一个中文分词的例子")
        .map(|l| l.text().to_string())
        .collect();
    println!("{:?}", words); // ["这是", "一个", "中文", "分词", "例子"]
}
```

> ⚠️ **Do not use the deprecated `IKSegmenter::new(text, use_smart)`**: it rebuilds dictionaries on every call. Always go through `TokenizerFactory`.

### Python

```bash
pip install ik-analyzer-py
```

Building from source (requires `maturin`): `cd python && maturin develop --release`.

```python
import ik_analyzer as ik

tok = ik.Tokenizer()                       # smart=True by default
print(tok.segment("欢迎使用本分词器"))     # ['欢迎', '使用', '本', '分词器']

cfg = ik.Config()
cfg.use_smart(False)                       # fine-grained
print(ik.Tokenizer(cfg).segment("张三说的确实在理"))
```

See [docs/USAGE.en.md](https://github.com/hoare211/ik-analyzer-rs/blob/main/docs/USAGE.en.md) for `cut` with offsets, `segment_batch` (releases the GIL) and every `Config` method.

### CLI

```bash
cargo run --bin ik-analyzer -- "研究生命起源"
cargo run --bin ik-analyzer -- "测试自定义词" --custom-dict custom/long_word_dict.dic
```

## 📊 Performance

Arbitration was optimized from O(n²) to an O(n) chain model (`judge-fast`, enabled by
default). Reproduce the measurements with:

```bash
cargo run --release --example perf_cmp -- benches/corpus.txt
```

> ⚠️ **About comparisons with the original**: this repository's own measurements
> (`benches/corpus.txt`, 288,712 characters / 202,231 tokens, release) are reproducible,
> e.g. with `cargo run --release --example alloc_count`. However, **no same-machine,
> same-corpus Java comparison has been performed** — the ~21 ms / ~31 ms figures in
> historical documents are the original 2012 documentation values (different machine and
> corpus) and are only useful as an order-of-magnitude reference, not as a same-machine
> conclusion. This README therefore makes no "beats the original by X ms" claim; measure
> it yourself on your own hardware and corpus.
>
> **Reproducible scope of the correctness evidence**: `cargo test` can be re-run directly
> (including the 405-line long-text parity golden set × 2 modes). The Java-side A/B harness
> lives in [`testing/ab/`](https://github.com/hoare211/ik-analyzer-rs/blob/main/testing/ab/README.md) and needs your own IK Analyzer 2012_u6 and
> analysis-ik source trees (both Apache-2.0). The 4×10⁶ randomized differential and
> 9133-line corpus runs were produced by that harness with 0 divergences.

## 🏗️ Architecture

```
TokenizerFactory (loads dictionaries once, shares Arc<Dictionary>)
  └── IKSegmenter (one per text, lightweight, Iterator<Item=Lexeme>)
        ├── AnalyzeContext (double-buffered, streaming)
        ├── CJKSegmenter / CnQuantifierSegmenter / LetterSegmenter
        └── IKArbitrator (ambiguity arbitration)
```

## 📚 Documentation

- **[Usage manual](https://github.com/hoare211/ik-analyzer-rs/blob/main/docs/USAGE.en.md)** — installation, configuration, both modes, `Lexeme`, streaming, errors, concurrency, Python bindings, FAQ.
- **[Contributing guide](https://github.com/hoare211/ik-analyzer-rs/blob/main/CONTRIBUTING.md)** — development environment, test matrix, commit and PR conventions.
- **[中文使用手册](https://github.com/hoare211/ik-analyzer-rs/blob/main/docs/使用手册.md)** — Chinese edition.

## ⚠️ Important notes

- Use `TokenizerFactory`; do not use the deprecated `IKSegmenter::new`.
- Stop words are removed from the final output (they do not form standalone tokens), matching the original.
- **u6 parity is only promised when `Config::u6_parity(use_smart)` is explicitly enabled**.
  Defaults are independent product decisions (smart + case-insensitive + full-width→half-width +
  bundled stop words + `AstralMode::Ideographs`). All differences from u6 are configurable,
  disabled by default, or disclosed:
  - **u6 defect fixes** (we differ): u6 re-emits one overlapping token at every page boundary
    for inputs >4096 characters; u6 pollutes the first entry of dictionaries with a BOM (we strip
    it); `sat_mul` saturation (u6's `int` multiplication can silently overflow, unreachable with
    its dictionaries).
  - **Representation**: `offset` is measured in **code points**, not directly comparable with
    Java's UTF-16 code units (the Python binding uses byte offsets); token text/type sequences
    are unaffected.
  - **Configurable improvements**: CJK Extension B–I ideographs are recognized by default
    (`AstralMode::Drop` restores the u6 regime); other supplementary-plane characters such as
    emoji are dropped by default (`AstralMode::Tokens` emits them).
  - The bundled stop-word list is loaded by default (33 English entries = u6 distribution /
    analysis-ik default, byte-identical to u6's `stopword.dic`, plus 69 Chinese entries added by
    this project); `Config::u6_parity` empties it.
- Differences and improvements versus the original are documented in
  [Usage manual §6.4](https://github.com/hoare211/ik-analyzer-rs/blob/main/docs/USAGE.en.md#64-behavioral-parity-contract-with-ik-2012_u6).

## Compatibility / MSRV

- Requires **Rust 1.85+** (`edition = "2024"`). `Cargo.toml` declares `rust-version = "1.85"`.
- `judge-fast` is enabled by default (O(n) chain-model arbitration). `--no-default-features` falls back to `judge_reference` (O(n²), byte-identical to the pre-feature behavior, for emergency rollback or offline comparison).
- The CLI logging backend is optional: only `cargo run --bin ik-analyzer --features env_logger -- "..."` prints the dictionary loading report.

## Contributing

Issues and PRs are welcome. Please read [CONTRIBUTING.md](https://github.com/hoare211/ik-analyzer-rs/blob/main/CONTRIBUTING.md) first; report
security issues privately as described in [SECURITY.md](https://github.com/hoare211/ik-analyzer-rs/blob/main/SECURITY.md).

## License

[Apache-2.0](https://github.com/hoare211/ik-analyzer-rs/blob/main/LICENSE). The algorithms and bundled dictionaries originate from the IK Analyzer
family: [IK Analyzer 2012_u6](https://code.google.com/p/ik-analyzer/) provides the algorithm
lineage, dictionaries and the behavior-compatibility baseline, while
[analysis-ik](https://github.com/infinilabs/analysis-ik) is the implementation baseline (both
Apache-2.0). See [NOTICE](https://github.com/hoare211/ik-analyzer-rs/blob/main/NOTICE) for the complete third-party attribution.
