# ik-analyzer-rs Usage Manual

> Applies to **ik-analyzer-rs 1.3.0**. All API names and signatures are verified against the
> source (`src/`, `python/src/`, `src/bin/`).
> This library is an independent implementation built on the analysis-ik baseline; its default
> output reflects product decisions (and may improve on the original). Compatibility with
> IK Analyzer 2012_u6 is an **opt-in** contract described in §6.4, not the default regime.

**English** | [简体中文](使用手册.md)

---

## Table of contents

1. [Overview and terminology](#1-overview-and-terminology)
2. [Installation](#2-installation)
3. [Core concepts](#3-core-concepts)
4. [Quick start in Rust](#4-quick-start-in-rust)
5. [Two segmentation modes](#5-two-segmentation-modes)
6. [Configuration](#6-configuration)
   - [Behavioral parity contract with IK 2012_u6](#64-behavioral-parity-contract-with-ik-2012_u6)
   - [AstralMode: supplementary-plane characters](#65-astralmode)
7. [Lexeme output and token types](#7-lexeme-output-and-token-types)
   - [Zero-copy entry `segment_spans`](#71-zero-copy-entry-segment_spans)
8. [Streaming and large files](#8-streaming-and-large-files)
9. [Error handling](#9-error-handling)
   - [Detecting streaming read failures](#91-detecting-streaming-read-failures)
10. [Multithreading and concurrency](#10-multithreading-and-concurrency)
11. [Performance best practices](#11-performance-best-practices)
12. [Command-line tool](#12-command-line-tool)
13. [Python bindings](#13-python-bindings)
14. [Custom dictionary format](#14-custom-dictionary-format)
15. [Feature flags](#15-feature-flags)
16. [Dictionary loading, cold start and logging](#16-dictionary-loading-cold-start-and-logging)
17. [Arbitrary `Read` sources and chunk invariance](#17-arbitrary-read-sources-and-chunk-invariance)
18. [Dictionary API and benchmarks](#18-dictionary-api-and-benchmarks)
19. [Examples and benches](#19-examples-and-benches)
20. [Custom stop words](#20-custom-stop-words)
21. [FAQ](#21-faq)

---

## 1. Overview and terminology

ik-analyzer-rs inherits IK Analyzer's algorithm structure but is **not a line-by-line port of
u6**: the implementation baseline is analysis-ik (its defect fixes are absorbed), defaults are
chosen for product needs, and only an explicit `Config::u6_parity` switches to the
u6-compatible regime (see §6.4).

| Term | Meaning |
|---|---|
| **TokenizerFactory** | Tokenizer factory. Loads dictionaries **once**, then cheaply creates any number of segmenters. The recommended entry point. |
| **IKSegmenter** | A segmenter instance implementing `Iterator<Item = Lexeme>`, yielding tokens one by one. |
| **Config / ConfigBuilder** | Segmentation configuration (smart mode, dictionaries). `ConfigBuilder` provides a chained API. |
| **DictSource** | Dictionary source: either a file `DictSource::File(path)` or in-memory text `DictSource::Text(text)`. |
| **Lexeme** | One token: text, start/end positions, token type. |
| **smart / fine modes** | smart = ambiguity resolution (coarse-grained, closer to semantics); fine = fine-grained (as many splits as possible). |
| **Extension vs main dictionary** | The main dictionary is the bundled IK word list; `add_ext_*` **appends** to it without changing the default main list. |

---

## 2. Installation

### 2.1 Rust library

Add to `Cargo.toml`:

```toml
[dependencies]
ik-analyzer-rs = "1.3.0"
```

> Note: the `[lib]` name inside the repository is `ik_analyzer`, while the published crate name
> is `ik-analyzer-rs`. Import with `use ik_analyzer::...;` (the lib name).

### 2.2 Python bindings

See [§13](#13-python-bindings). Install from PyPI:

```bash
pip install ik-analyzer-py
```

Building from source requires `maturin`:

```bash
cd python
maturin develop --release        # install into the current virtual environment
# or build a wheel:
maturin build --release
```

### 2.3 Command-line tool

See [§12](#12-command-line-tool); you can run it directly with `cargo run`.

---

## 3. Core concepts

```
ConfigBuilder ──build()──▶ Config
                              │
                   TokenizerFactory::new(config)
                              │   (only this step loads dictionaries; expensive)
                              ▼
                   TokenizerFactory  (shares Arc<Dictionary>)
                              │
            ┌─────────────────┼───────────────────────┐
            ▼                 ▼                        ▼
   factory.segmenter(text)  factory.segmenter_from_read(r)   factory.dictionary()
            │   (very cheap, once per text)
            ▼
      IKSegmenter (Iterator<Item = Lexeme>)
```

**The crucial point**: create `TokenizerFactory` **once at application startup** (it reads
dictionaries and builds tries, which is expensive); afterwards calling
`factory.segmenter(text)` per text is very cheap.

---

## 4. Quick start in Rust

```rust
use ik_analyzer::{Config, TokenizerFactory};

fn main() {
    // 1) Build the factory with default settings (use_smart = true by default)
    let factory = TokenizerFactory::new(Config::builder().use_smart(true).build());

    // 2) Segment text; you get an iterator
    let mut seg = factory.segmenter("欢迎使用本分词器");

    // 3) Pull tokens one by one
    assert_eq!(seg.next().unwrap().text(), "欢迎");
    assert_eq!(seg.next().unwrap().text(), "使用");

    // Or collect into Vec<String> at once
    let words: Vec<String> = factory
        .segmenter("中华人民共和国")
        .map(|l| l.text().to_string())
        .collect();
    println!("{:?}", words); // ["中华人民共和国"]
}
```

`IKSegmenter` implements `Iterator`, so you can also write
`for lexeme in factory.segmenter(text) { ... }`.

---

## 5. Two segmentation modes

Controlled by `ConfigBuilder::use_smart(bool)`:

```rust
let smart = TokenizerFactory::new(Config::builder().use_smart(true).build());
let fine  = TokenizerFactory::new(Config::builder().use_smart(false).build());

let text = "张三说的确实在理";

let s: Vec<String> = smart.segmenter(text).map(|l| l.text().to_string()).collect();
let f: Vec<String> = fine.segmenter(text).map(|l| l.text().to_string()).collect();

println!("smart: {:?}", s); // ["张三", "说的", "确实", "在理"]
println!("fine : {:?}", f); // ["张三", "三", "说的", "的确", "确实", "实在", "在理"]
```

- **smart (default)**: ambiguity resolution, producing coarse-grained results closer to semantics.
- **fine**: keeps every possible split, suitable for indexing and high-recall scenarios.

Both modes match the original IK Analyzer's segmentation quality (see §6.4).

---

## 6. Configuration

### 6.1 `DictSource`: dictionary sources

```rust
pub enum DictSource {
    File(PathBuf),            // load from a file
    Text(Cow<'static, str>),  // load from an in-memory string (one word per line)
}
// Convenience constructors
DictSource::text("词1\n词2")          // &'static str, zero-copy
DictSource::text_owned("动态".into()) // runtime String
```

`Text` sources let unit tests, embedded use cases and dynamic dictionaries inject words
**without touching the filesystem**.

### 6.2 `ConfigBuilder` methods

> `ConfigBuilder` is **chained**: every setter returns `self`, so calls can be chained and
> finished with `.build()`.

| Method | Purpose |
|---|---|
| `use_smart(bool)` | Segmentation mode (default `true`, same as v1.0.0 / 2012_u6) |
| `enable_lowercase(bool)` | Case policy (default `true` = ASCII case-insensitive; `false` = strictly case-sensitive) |
| `astral_mode(AstralMode)` | Supplementary-plane handling (default `AstralMode::Ideographs`, see §6.5) |
| `with_main_dict(Vec<DictSource>)` | **Replace** the whole main dictionary (overrides the bundled IK word list) |
| `with_quantifier(Vec<DictSource>)` | **Replace** the whole quantifier dictionary |
| `with_stop_words(Vec<DictSource>)` | **Replace** the whole stop-word dictionary |
| `add_ext_dict_paths(Vec<PathBuf>)` | Append extension entries to the main dictionary (file paths) |
| `add_ext_dict_text(&str)` | Append extension entries to the main dictionary (static text) |
| `add_ext_dict_owned_text(String)` | Append extension entries to the main dictionary (runtime text) |
| `add_ext_stop_word_paths / _text / _owned_text` | Append extension stop words (same three forms) |
| `add_ext_quantifier_paths / _text / _owned_text` | Append extension quantifiers (same three forms) |

> **Key point**: `with_*` **replaces** the corresponding default dictionary; `add_ext_*`
> **appends** to it. Most customizations only need `add_ext_*`, keeping the bundled IK main
> word list.
>
> **A separate full-configuration constructor** exists: `Config::u6_parity(use_smart)` — a
> frozen IK 2012_u6 regime preset, see §6.4.

### 6.3 Examples

**Append a custom dictionary file:**

```rust
let factory = TokenizerFactory::new(
    Config::builder()
        .use_smart(true)
        .add_ext_dict_paths(vec![PathBuf::from("custom/long_word_dict.dic")])
        .build(),
);
```

**Use in-memory dictionaries only (no files, no bundled IK main list):**

```rust
let factory = TokenizerFactory::new(
    Config::builder()
        .use_smart(true)
        .with_main_dict(vec![DictSource::text("深度学习\n自然语言处理")])
        .build(),
);
```

**Append in-memory stop words:**

```rust
let factory = TokenizerFactory::new(
    Config::builder()
        .add_ext_stop_word_owned_text("的\n了\n嘛".to_string())
        .build(),
);
```

**Mixed (files + memory):**

```rust
let factory = TokenizerFactory::new(
    Config::builder()
        .add_ext_dict_paths(vec![PathBuf::from("dict/a.dic"), PathBuf::from("dict/b.dic")])
        .add_ext_dict_owned_text("临时热词".to_string())
        .build(),
);
```

> Dictionary files are read line by line, one word per line. Whitespace trimming follows
> u6's `String.trim()` (code points ≤ U+0020) **plus U+00A0 (NBSP)**, and a leading UTF-8 BOM
> (`U+FEFF`, common in Windows-edited `.dic` files) is stripped — u6 does not strip it and
> pollutes the first entry. Note that **other Unicode whitespace such as U+3000 is not
> trimmed** (matching u6; trimming it would silently change the entry set). Case handling is
> applied at **DAWG build time** according to `enable_lowercase` (ASCII normalization, lowercase
> by default). Invalid lines are skipped with a `warn` log instead of panicking (see
> `src/dic/dictionary.rs`).

### 6.4 Behavioral parity contract with IK 2012_u6

**This section describes an opt-in compatibility capability, not the library's default
positioning.**

| Layer | Positioning |
|---|---|
| Implementation | Based on **analysis-ik** (`master@6d2d70f`): its defect fixes relative to u6 are absorbed, and its behavioral changes are exposed as configurable mechanisms |
| Behavior (default output) | Independent product decisions, **may improve on u6** (bundled stop words, CJK Extension B–I, ...) |
| Compatibility (opt-in) | `Config::u6_parity` provides an explicit contract for downstream users that need exact u6 behavior |

> **Parity contract**: **only when `Config::u6_parity` is explicitly enabled**, for the same
> "effective configuration" (`useSmart` × case policy × stop-word set × dictionary set × astral
> mode), the token sequence (text + type + relative order) is identical to u6.
> The contract is **not** guaranteed by coincidence of defaults; it rests on: (1) the explicit
> preset `Config::u6_parity`; (2) the mapping table below; (3) golden differential tests
> (harness in `testing/ab/`). The default configuration does **not** promise this equivalence.

**Configuration mapping** (each u6 assembly ↔ this library's configuration):

| u6 assembly | Corresponding configuration |
|---|---|
| bare core (no cfg.xml, no stop words) | `Config::u6_parity(use_smart)` |
| jar distribution (`ext.dic` + 32 English stop words) | default configuration + `with_stop_words(contents of u6's `stopword.dic`)` |

```rust
// IK 2012_u6 regime: u6 distribution dictionaries + no stop words + case-insensitive + astral dropped
let factory = TokenizerFactory::new(Config::u6_parity(true));
```

`u6_parity` pins a **frozen** configuration: main dictionary = u6 distribution word lists
(`main2012.dic` + `ext.dic` + `quantifier.dic`, referenced directly through `u6_dict_sources()`
so future expansion of the default lists cannot break it), stop words emptied,
`enable_lowercase=true`, `AstralMode::Drop`. If you need "u6 plus custom words", assemble it
explicitly with `with_main_dict` and take responsibility for parity yourself.

**Differences between defaults and u6** (all configurable, disabled by default, or disclosed):

| Dimension | Default behavior | Relation to u6 |
|---|---|---|
| `use_smart` | `true` | u6's core constructor requires it and has no default; both smart/fine regimes are covered by 0-divergence evidence |
| `enable_lowercase` | `true` | = u6's unconditional ASCII lowercasing |
| Stop words | 102 bundled entries filtered by default (33 English = u6 distribution / analysis-ik default, byte-identical to u6's `stopword.dic`; 69 Chinese = project enhancement) | Product enhancement; `u6_parity` / `with_stop_words(vec![])` empties it to the u6 regime |
| Astral characters | `AstralMode::Ideographs`: CJK Extension B–I recognized as ideographs, emoji dropped | u6 drops all of them due to Java `char`; `AstralMode::Drop` restores the u6 regime (see §6.5) |
| CJK gating | Advance dictionary matching for anything that is not `CHAR_USELESS` (9 mixed entries such as `A股`/`K线` become reachable) | Matches u6 / analysis-ik (fix from the 2026-09-27 issue record §1) |
| Full-width uppercase `regularize` | else-if: `Ａ` → `A`, no additional lowercasing | Byte-identical to u6 / analysis-ik (fix from issue record §2) |
| Dictionary-side case | ASCII folding at build time, tied to `enable_lowercase` | u6 uses Unicode `toLowerCase()`; with the bundled dictionaries both regimes are **unobservable** (audit of 276,135 entries: no entry where Unicode lowercase ≠ ASCII folding) |

**Exception list** (u6's own defects or representation-layer differences; all disclosed; these
fixes/differences are not treated as parity violations):

| # | Exception | Nature |
|---|---|---|
| 1 | Inputs >4096 characters: u6 re-emits one overlapping token at each page boundary; this library does not | u6 defect fix (A-level measurement) |
| 2 | Dictionaries with a BOM: u6 pollutes the first entry; this library strips the leading `U+FEFF` | u6 defect fix |
| 3 | `sat_mul` path weight multiplication: u6's chained `int` multiplication can silently overflow; this library saturates at `i32::MAX` | Robustness only; unreachable with u6 dictionaries |
| 4 | `offset` unit: code points here, UTF-16 code units in Java (byte offsets in the Python binding) | Representation layer; token text/type sequences match; differential tools must convert (BMP 1:1, astral 1→2) |
| 5 | Chained quantifier merging (two independent `if`s in `compound`): u6 mistakenly uses `if/else if` and misses a merge | u6 defect fix (`42三首` merges) |
| 6 | Dictionary line trimming: besides u6's `trim()` regime (≤U+0020), **U+00A0 (NBSP) is also trimmed**; entries with a trailing NBSP (e.g. `方正凤凰城\u{a0}{4}`) are unreachable in u6 but reachable here. Other Unicode whitespace such as U+3000 is **not** trimmed (= u6; trimming would silently expand the entry set) | Intentional improvement (declared in issue 20261012 §7.4; the U+3000 behavior was fixed on 2026-10-12) |

**Comparison results** (9133-line corpus × {smart, fine} × {default, Java regime}, four
assemblies, 0 divergences; assembly details in the issue record *大小写全角与词典匹配差异* §4/§5):

- **Fine-grained mode: 0 divergences across all 9133 lines** (`segment_dump --smart false --no-stop-words`);
- **smart mode: exactly 1 divergent line** (line 5128), which is the "two-round quantifier
  merging" item above — caused by a defect in u6 itself;
- Under defaults (stop words enabled), differences from the u6 bare-core regime are **100%
  explained by filtered stop-word tokens**; there is no other semantic drift.

```bash
# Command-line comparison (one line of tokens per input line, for diffing)
cat benches/corpus.txt | cargo run --release --example segment_dump -- --no-stop-words
```

### 6.5 `AstralMode`

Supplementary-plane (astral, code points > U+FFFF) characters include emoji, symbols and CJK
Extension B–I ideographs. Both u6 and analysis-ik operate on Java `char` (UTF-16 code units):
u6 drops all of them; analysis-ik emits them through `SurrogatePairSegmenter` (with two proven
defects — a single ENGLISH letter adjacent to a surrogate pair is lost, and a surrogate pair
spanning a page boundary is split in half). This library works directly on Rust `char`
(scalar values), and behavior is selected explicitly by a three-value enum:

| Variant | Behavior |
|---|---|
| `AstralMode::Drop` | Drop all non-BMP characters (**= strict u6 regime**; the value pinned by `Config::u6_parity`) |
| `AstralMode::Ideographs` | **Default**: CJK Extension B–I recognized as ideographs (can participate in dictionary matching); everything else (emoji, ...) dropped |
| `AstralMode::Tokens` | Emit every non-BMP character as a single-character `CNCHAR` token (mechanism aligned with analysis-ik's goal, without copying its defects) |

```rust
let factory = TokenizerFactory::new(
    Config::builder().astral_mode(AstralMode::Tokens).build(),
);
// "你好😀世界" => ["你好", "😀", "世界"]
```

**Implementation guarantees** (improvements over analysis-ik, all backed by A-level
measurements):
1. A single ENGLISH letter adjacent to an astral character is not lost (analysis-ik loses it);
2. Astral characters are never split when crossing the 4096-character page boundary
   (analysis-ik emits the high surrogate as a dirty token; the Rust `char` model avoids this
   structurally, locked in by `test_astral_across_buffer_boundary`);
3. Astral characters can participate in dictionary matching (analysis-ik's
   `SurrogatePairSegmenter` performs no dictionary matching at all).

---

## 7. `Lexeme` output and token types

`IKSegmenter` yields `Lexeme`, not just strings. For position/type information, use the
`Lexeme` methods:

| Method | Returns | Meaning |
|---|---|---|
| `text()` | `&str` | Token text |
| `begin_position()` | `i32` | Absolute start position (character offset) |
| `end_position()` | `i32` | Absolute end position |
| `length()` | `i32` | Token length (characters) |
| `offset()` | `i32` | Buffer offset (for multi-buffer scenarios) |
| `lexeme_type()` | `i32` | Token type constant (see below) |

**Token type constants** (one-to-one with the original IK):

```rust
Lexeme::TYPE_UNKNOWN   = 0
Lexeme::TYPE_ENGLISH   = 1
Lexeme::TYPE_ARABIC    = 2
Lexeme::TYPE_LETTER    = 3
Lexeme::TYPE_CNWORD    = 4    // Chinese word
Lexeme::TYPE_CNCHAR    = 64   // single Chinese character
Lexeme::TYPE_OTHER_CJK = 8    // other CJK (Japanese/Korean, ...)
Lexeme::TYPE_CNUM      = 16   // Chinese numeral
Lexeme::TYPE_COUNT     = 32   // quantifier (e.g. "个", "斤")
Lexeme::TYPE_CQUAN     = 48   // Chinese numeral + quantifier
```

**Example with positions and types:**

```rust
for lex in factory.segmenter("张三123公斤") {
    println!(
        "[{}] begin={} end={} type={}",
        lex.text(),
        lex.begin_position(),
        lex.end_position(),
        lex.lexeme_type(),
    );
}
```

> Rare characters (including supplementary-plane characters such as `𪚥` U+2A6A5) are
> recognized as ideographs (CJK) instead of being split into surrogate halves and dropped as in
> the original (see the exception list in §6.4).

### 7.1 Zero-copy entry `segment_spans`

`segmenter()` allocates one `String` **per emitted token** to produce its text. Measurements
show this accounts for **94% of all heap allocations** on the segmentation hot path
(`benches/corpus.txt`: 288,712 characters / 202,231 tokens → about 202,231 of 205,307
allocations; verify with `examples/alloc_count.rs`).

If you only need positions and types (slicing, indexing, offset mapping, ...), use
`segment_spans()`:

```rust
use ik_analyzer::{Config, TokenizerFactory, char_to_byte_map};

let factory = TokenizerFactory::new(Config::builder().build());
let text = "欢迎使用本分词器";

// Build the char→byte map once for the whole text and reuse it for all spans
let map = char_to_byte_map(text);

let words: Vec<&str> = factory
    .segment_spans(text)              // yields Span { begin, end, lexeme_type }
    .map(|s| s.slice(text, &map))     // zero-allocation slice of the original text
    .collect();
assert_eq!(words, ["欢迎", "使用", "本", "分词器"]);
```

`Span` is a `Copy` position range: `begin()` / `end()` / `length()` / `lexeme_type()`, with
**global `char` indices** (continuous across buffer pages). The token sequence is **identical**
to `segmenter()` (same advancement logic, same stop-word filtering and quantifier merging).

| | `segmenter()` | `segment_spans()` |
|---|---|---|
| Output | `Lexeme` (with `String` text) | `Span` (positions + type only) |
| Heap allocations per token | 1 | **0** |
| Measured allocations | 205,307 (0.711/char) | **2,959 (0.010/char)** |
| Measured allocated bytes | 11.79 MiB (42.8/char) | 10.24 MiB (37.2/char) |
| Measured time | 39.95 ms | **37.76 ms** |

> ⚠️ **Text semantics differ**: `Span::slice` returns the **original** substring, while
> `Lexeme::text()` returns **normalized** text (`regularize`: full-width→half-width, plus ASCII
> lowercasing when `enable_lowercase` is on). For input `"IKAnalyzer2012"`, the slice is
> `IKAnalyzer2012` while `text()` is `ikanalyzer2012`. **Positions and types are identical.**
> If downstream code needs normalized text, use `segmenter()`.

---

## 8. Streaming and large files

For files, network streams and other inputs that cannot fit in memory, use
`TokenizerFactory::segmenter_from_read`:

```rust
use std::fs::File;

let file = File::open("big_corpus.txt")?;
let mut seg = factory.segmenter_from_read(file);

for lexeme in seg {
    // Process token by token, constant memory
    println!("{}", lexeme.text());
}
```

- Incremental UTF-8 decoding correctly handles **multi-byte characters split across buffer boundaries**;
- Invalid UTF-8 bytes are replaced with `U+FFFD` instead of panicking (`src/core/char_source.rs`);
- The reader must satisfy `Read + Send + 'static` (so factories can be shared across threads).

---

## 9. Error handling

By default, `TokenizerFactory::new` **panics** when a dictionary file does not exist. In
production, use `try_new` for graceful handling:

```rust
use ik_analyzer::{Config, TokenizerFactory, IkError};

let result = TokenizerFactory::try_new(
    Config::builder()
        .add_ext_dict_paths(&["maybe_missing.dic"])
        .build(),
);

match result {
    Ok(factory) => { /* use the factory */ }
    Err(IkError::FileNotFound(path)) => {
        eprintln!("dictionary not found: {}", path.display());
    }
    Err(IkError::Io(e)) => { eprintln!("I/O error: {}", e); }
    Err(IkError::InvalidEncoding(_)) => { eprintln!("dictionary is not UTF-8"); }
    _ => {}
}
```

Error types distinguish `Io` / `FileNotFound` / `InvalidEncoding` (see `src/error.rs`), which is
more controllable than the original's `RuntimeException` or silent swallowing.

### 9.1 Detecting streaming read failures

`Iterator<Item = Lexeme>` cannot return errors, so a streaming segmentation can only stop on a
**read failure** and return what was read so far — i.e. **the content is truncated**. Check for
this explicitly:

```rust
use ik_analyzer::{Config, TokenizerFactory};
use std::fs::File;

let factory = TokenizerFactory::new(Config::builder().build());
let mut seg = factory.segmenter_from_read(File::open("huge.log")?);

let tokens: Vec<String> = seg.by_ref().map(|l| l.text().to_string()).collect();
if let Some(e) = seg.take_error() {
    // Read failure: `tokens` may be incomplete; decide per your requirements
    eprintln!("read failed (content may be truncated): {e}");
}
```

`seg.has_error()` is also available for a read-only query (repeatable). An error is additionally
logged with `log::error!`, but **do not rely on logs alone** — `env_logger` is an optional
feature, and downstream crates have no log output by default.

---

## 10. Multithreading and concurrency

`TokenizerFactory` shares immutable dictionaries through `Arc<Dictionary>`, so it can be
**cloned and reused across threads**:

```rust
use std::thread;

let factory = TokenizerFactory::new(Config::builder().build());
let mut handles = vec![];

for _ in 0..4 {
    let f = factory.clone(); // cheap: clones only the Arc
    handles.push(thread::spawn(move || {
        let words: Vec<String> = f.segmenter("并发分词测试").map(|l| l.text().to_string()).collect();
        words.len()
    }));
}

for h in handles { let _ = h.join(); }
```

- `TokenizerFactory: Clone` (clones only the `Arc`), so threads share one dictionary with
  **zero copies and no lock contention**;
- Each thread's `IKSegmenter` created by `factory.segmenter(text)` is thread-local and independent;
- Multiple factories with **different dictionaries/configurations** can coexist in one process
  (`Arc` isolation), which the original's global singleton cannot do.

---

## 11. Performance best practices

1. **Create the factory once**: `TokenizerFactory::new` loads and compiles all dictionaries
   (expensive); `segmenter()` is cheap. Keep the factory at the top of your application's
   lifecycle or in a global (`lazy_static` / `OnceCell`).
2. **Reuse the factory for batches**: one factory can serve unlimited `segmenter()` calls;
   dictionaries are built only in `new`.
3. **Use streaming for large texts**: prefer `segmenter_from_read` for inputs larger than a few MB.
4. **`judge-fast` is the default**: the default feature enables O(n) chain-model arbitration;
   smart mode takes about 21 ms, fine mode is on par with the original (re-measure with
   `examples/perf_cmp.rs`). To fall back to `judge_reference` (byte-identical to the original
   behavior), build with `--no-default-features`.
5. **Do not** use the deprecated `IKSegmenter::new(text, use_smart)`: it rebuilds dictionaries
   on every call (marked `#[deprecated]` in the source). Always use `TokenizerFactory`.

---

## 12. Command-line tool

The repository ships a demo CLI (`src/bin/ik-analyzer.rs`) that you can run directly:

```bash
# Defaults follow v1.0.0 / 2012_u6 semantics: smart + case-insensitive (ASCII) + full-width→half-width
cargo run -- "这是一个中文分词的例子"
# Output: 这是/一个/中文/分词/例子

# Fine-grained (--no-smart)
cargo run -- "研究生命起源" --no-smart
# Output: 研究生/研究/生命/起源

# Preserve original case (strictly case-sensitive, aligned with analysis-ik 5.0 enable_lowercase=false)
cargo run -- "HelloWorld 中文" --lowercase false
# Output: HelloWorld/中文

# Append a custom dictionary
cargo run -- "测试自定义词" --custom-dict custom/long_word_dict.dic
```

Arguments:

| Argument | Description |
|---|---|
| `text` (positional, required) | Text to segment |
| `--no-smart` | Use fine-grained mode (smart by default, same as v1.0.0) |
| `--lowercase <true\|false>` | Case policy (default `true` = case-insensitive, same as v1.0.0; `false` = preserve case) |
| `--custom-dict <path>` | Append one extension main-dictionary file |

> The CLI prints a dictionary loading report at startup (requires `env_logger`). It is intended
> for demos and quick verification; use the library API for real integrations.

---

## 13. Python bindings

### 13.1 Installation

Install from PyPI (recommended):

```bash
pip install ik-analyzer-py
```

Building from source:

```bash
cd python
pip install maturin
maturin develop --release     # build and install into the current environment
# or
maturin build --release       # produce a wheel in target/wheels/
pip install target/wheels/*.whl
```

> Note: the PyPI package name is `ik-analyzer-py`, but the import name is `ik_analyzer`
> (`import ik_analyzer`), because the underlying cdylib is named `ik_analyzer`.

### 13.2 Basic usage

```python
import ik_analyzer as ik

# 1) Default configuration (smart=True)
tok = ik.Tokenizer()                       # defaults when no config is passed
print(tok.segment("欢迎使用本分词器"))
# ['欢迎', '使用', '本', '分词器']

# 2) Custom configuration
cfg = ik.Config()
cfg.use_smart(False)                       # fine-grained
tok2 = ik.Tokenizer(cfg)
print(tok2.segment("张三说的确实在理"))
# ['张三', '三', '说的', '的确', '确实', '实在', '在理']
```

### 13.3 `Config` methods (one-to-one with Rust)

| Python method | Meaning |
|---|---|
| `use_smart(bool)` | Segmentation mode (default True, same as v1.0.0 / 2012_u6) |
| `enable_lowercase(bool)` | Case policy (default True = case-insensitive; False = preserve case) |
| `astral_mode(AstralMode)` | Supplementary-plane handling (default `AstralMode.Ideographs`, see §6.5) |
| `with_main_dict_from_paths([...])` | Replace the whole main dictionary (file paths) |
| `with_main_dict_from_sources([{...}])` | Replace the whole main dictionary (detailed sources) |
| `with_quantifier_from_paths / _from_sources` | Replace the quantifier dictionary |
| `with_stop_words_from_paths / _from_sources` | Replace the stop-word dictionary |
| `add_ext_dict_from_paths([...])` | Append extension main entries (files) |
| `add_ext_dict_from_text("词1\n词2")` | Append extension main entries (text) |
| `add_ext_stop_word_from_paths / _from_text` | Append extension stop words |
| `add_ext_quantifier_from_paths / _from_text` | Append extension quantifiers |

`with_*_from_sources` / `add_ext_*_from_sources` take a list of dicts, each containing either
`{"path": "..."}` or `{"text": "..."}` (mutually exclusive):

```python
cfg = ik.Config()
cfg.with_main_dict_from_sources([{"path": "dict/main.dic"}, {"text": "热词A\n热词B"}])
tok = ik.Tokenizer(cfg)
```

**`AstralMode` and the u6 parity preset** (corresponds to Rust §6.4 / §6.5):

```python
# Three-value enum: Drop (= strict u6 regime) / Ideographs (default) / Tokens
cfg = ik.Config()
cfg.astral_mode(ik.AstralMode.Tokens)      # emit emoji as tokens too
tok = ik.Tokenizer(cfg)
tok.segment("你好😀世界")                    # ['你好', '😀', '世界']

# IK 2012_u6 parity: pin u6 distribution dictionaries + no stop words + case-insensitive + astral dropped
p = ik.Tokenizer.u6_parity(use_smart=True)
p.segment("这是一个中文分词的例子")          # ['这是', '一个', '中文', '分词', '的', '例子'] ('的' is not filtered)
```

### 13.4 Three segmentation methods

| Method | Returns | Description |
|---|---|---|
| `segment(text)` | `list[str]` | Token texts (most common) |
| `cut(text)` | `list[Lexeme]` | Token objects with positions/types (byte offsets) |
| `segment_batch(list[str])` | `list[list[str]]` | Batch segmentation, **releases the GIL** |

```python
# cut: byte offsets, convenient for mapping back to the original text
lexemes = tok.cut("张三123公斤")
for lm in lexemes:
    print(lm.text, lm.start, lm.end, lm.lexeme_type)
    # e.g.: 张三 0 6 CN_WORD

# segment_batch: many texts at once
result = tok.segment_batch(["第一条文本", "第二条文本", "第三条文本"])
# result is [[...], [...], [...]]
```

> Fields of the `Lexeme` returned by `cut`: `text: str`, `start: int`, `end: int`
> (**byte offsets**, not character offsets), `lexeme_type: str` (e.g. `"CN_WORD"`, `"ARABIC"`).

### 13.5 Version

```python
import ik_analyzer
print(ik_analyzer.__version__)   # '1.3.0' (core library version)
print(ik_analyzer.__binding_version__)  # '1.3.0' (this Python binding's version)
```

---

## 14. Custom dictionary format

Dictionaries are **plain text, UTF-8 encoded, one word per line**. The loader has **no comment
syntax**: a line starting with `#` is loaded as a literal entry, and only empty lines are
skipped.

```
深度学习
自然语言处理
ik分词器
```

- Case: controlled by `enable_lowercase` (default `true`, aligned with 2012_u6 behavior).
  Only **ASCII uppercase** letters are normalized at build time (not full Unicode lowercasing);
  the input side likewise only performs full-width→half-width plus ASCII lowercasing. With
  `enable_lowercase(false)` both dictionaries and input preserve case and matching is strictly
  exact (stop-word lists must then include case variants themselves).
  Example: `Config::builder().enable_lowercase(false).build()`.
- Source: `DictSource::File(path)` points at a `.dic`/`.txt` text file; `DictSource::Text(str)`
  supplies the string directly.
- Main, quantifier and stop-word dictionaries are independent, corresponding to
  `with_main_dict` / `with_quantifier` / `with_stop_words` and their `add_ext_*` variants.
- Bundled dictionaries live in `dict/` (`main2012.dic`, `quantifier.dic`,
  `ext_stop_word/stop.dic`). Prefer `add_ext_*` for custom words so the default main word list
  is left untouched.

---

## 15. Feature flags

Besides the defaults, Cargo exposes several feature flags (see `Cargo.toml`), controlled with
`--features` / `--no-default-features`:

| Feature | Default | Purpose |
|---|---|---|
| `judge-fast` | ✅ on | Enables the chain-model `judge_fast` as the production arbitration path (O(n), faster smart mode). |
| `judge-verify` | ❌ off | Keeps `judge_reference` and runs it against `judge_fast` for every real cross_path with a per-bit assert; any divergence panics. Verification only, with double-run cost. **Note: the differential tests themselves compile under plain `cargo test` and do not require this feature**; it is for diagnostic scenarios that force double-run assertions on real segmentation paths. |
| `env_logger` | ❌ off | Wires `env_logger` into the CLI (`ik-analyzer` bin) so the dictionary loading report becomes visible. CLI only; library users do not need it. |

Common combinations:

```bash
# Fall back to judge_reference, byte-identical to the original (disable the judge-fast default)
cargo build --no-default-features

# Force ref/fast double-run assertions on real paths (differential verification / debugging)
cargo run --features judge-verify --example judge_verify_stress -- benches/corpus.txt

# Quantify allocations (standing tool, no feature required)
cargo run --release --example alloc_count
cargo run --release --no-default-features --example alloc_count   # compare against judge_reference

# CLI dictionary loading report (requires the logging backend)
cargo run --release --features env_logger --bin ik-analyzer -- "研究生命起源"
```

> **Allocation accounting**: 20260930 once exposed `probe-count` / `probe-mimalloc` features for
> this, but neither ever had an implementation; they were removed as dead features without a
> replacement, leaving "allocation counts" unquantifiable (review P3-2). The standing
> `examples/alloc_count.rs` tool now covers it: `#[global_allocator]` lives inside the example,
> **requires no feature gate**, does not pollute downstream builds, and reserves no feature
> semantics in the published artifact. It reports both the `segmenter` and the `segment_spans`
> paths side by side.
>
> `env_logger` is for CLI diagnostics only and should not appear in production builds.

---

## 16. Dictionary loading, cold start and logging

### 16.1 Loading behavior (runtime DAWG construction)

The default main/stop-word/quantifier dictionaries are **plain-text `.dic` files embedded via
`include_str!`**. On the first `TokenizerFactory::new`, a compact DAWG (suffix-sharing directed
acyclic graph) is built **at runtime** from the text — **no precompilation step, no binary
dictionary artifacts, no build.rs**; the text is the single source of truth.

Cold-start cost (release, desktop-class CPU, 276k-entry main dictionary):

| Scenario | Time |
|---|---|
| First construction (first default dictionary in the process, all three) | ~62–75 ms (one-off) |
| Subsequent `TokenizerFactory::new` (default dictionaries, global cache hit) | ~1.6 µs |
| Custom dictionaries (`with_*` / `add_ext_*`) | Fully rebuilt per dictionary size, not cached |

Notes:

- Default dictionaries are shared per process by (dictionary kind × `enable_lowercase`)
  (global cache); multiple `TokenizerFactory` instances using the same default word lists build
  them only once.
- Custom sources are mutable in content and bypass the cache; every `new` rebuilds them. Reuse
  factory instances if you create custom-dictionary factories frequently.
- Peak memory during construction is ~52 MB, with ~4 MB live afterwards (the RSS reported by
  `/usr/bin/time -v` stays higher because the allocator retains freed pages; this is normal).

### 16.2 Dictionary loading report

The Rust library logs a **dictionary loading report** through the `log` crate (kind / source /
word count / status), for example:

```
[ik-analyzer-rs] Dictionary Loading Report:
  Main       | text sources (runtime-built DAWG)             | 275717 words | Loaded
  StopWord   | text sources (runtime-built DAWG)             |    101 words | Loaded
  Quantifier | text sources (runtime-built DAWG)             |    316 words | Loaded
```

On first construction the source column is `text sources (runtime-built DAWG)`; later cache
hits in the same process show `embedded text (cached DAWG)`. Word counts are the **deduplicated
real word counts** computed while building the DAWG (suffix sharing means node count ≠ word
count), matching the embedded text lists (the example above reflects the current bundled
dictionaries and changes as they are updated).

### 16.3 Logging backend

**Key point**: the report is emitted at `info!` level. If your application has no logging
backend installed, the report (and any `warn` about skipped invalid dictionary lines) **will not
appear**. Initialize a backend early in `main`:

```rust
use ik_analyzer::{Config, TokenizerFactory};

fn main() {
    env_logger::init();   // or an existing backend such as tracing
    let factory = TokenizerFactory::new(Config::builder().build());
    // ...
}
```

Add the dependency with `cargo add env_logger`.

---

## 17. Arbitrary `Read` sources and chunk invariance

`segmenter_from_read` accepts **any** source implementing `Read + Send + 'static`, not just
`File`:

```rust
use std::io::{Cursor, stdin, Read};
use std::net::TcpStream;

// In-memory string
let r1 = factory.segmenter_from_read(Cursor::new("内存中的文本".to_string().into_bytes()));
// Standard input
let r2 = factory.segmenter_from_read(stdin());
// Network stream (constant memory, suitable for large response bodies)
let stream = TcpStream::connect("127.0.0.1:8080").unwrap();
let r3 = factory.segmenter_from_read(stream);
```

**Chunk invariance (a streaming robustness guarantee)**: the internal `ReadCharSource` decodes
UTF-8 incrementally and correctly reassembles multi-byte characters that straddle `read`
boundaries. Therefore **regardless of how many bytes each `read` returns (including the extreme
case of 1 byte), the segmentation result is identical**. This is the correctness invariant of
the streaming path, verified in `examples/judge_verify_stress.rs` (which hard-asserts that
different chunk sizes produce the same segmentation fingerprint).

---

## 18. Dictionary API and benchmarks

`factory.dictionary()` returns an `Arc<Dictionary>` exposing low-level trie queries (such as
`match_in_main_dict(&[char], begin, length)`). This is **not needed for everyday segmentation**;
it exists for **benchmarks and dictionary-algorithm research**, e.g. measuring the cost of a
single longest-match step against the main dictionary:

```rust
use ik_analyzer::Dictionary;
use ik_analyzer::Config;

let dict = Dictionary::new(&Config::builder().build());
let chars: Vec<char> = "中华人民共和国".chars().collect();
// One main-dictionary match starting at position 0 (the smallest unit of CJKSegmenter's per-character longest match)
let hit = dict.match_in_main_dict(&chars, 0, 1);
```

> This API targets advanced users; in nearly all cases use `TokenizerFactory` + `segmenter()`
> instead of manipulating `Dictionary` directly.

---

## 19. Examples and benches

The repository includes runnable advanced cases suitable for performance baselines and
consistency checks:

| Path | Purpose |
|---|---|
| `examples/alloc_count.rs` | **Allocation accounting** (replaces the removed `probe-count`): counts heap allocations/bytes on the segmentation hot path, per-character and per-token metrics, and prints `segmenter` vs `segment_spans` (zero-copy) side by side. Usage: `cargo run --release --example alloc_count [corpus.txt]` |
| `examples/perf_cmp.rs` | Throughput benchmark: per-round time, character throughput and token rate for smart vs fine. Usage: `cargo run --release --example perf_cmp [corpus.txt]` |
| `examples/perf_trie.rs` | Trie lookup share: compares the single-step `match_in_main_dict` cost against full segmentation to quantify dictionary lookup overhead |
| `examples/judge_verify_stress.rs` | Consistency stress round: verifies **streaming invariance** (identical results across chunk sizes) and **concurrent determinism** (identical multi-threaded fingerprints) over ambiguous corpora plus custom-dictionary dimensions; with the `judge-verify` feature it also forces ref/fast double-run asserts on every cross_path |
| `examples/segment_dump.rs` | Line-by-line comparison tool: reads text from stdin and prints `/`-joined tokens per line, for `diff` against other implementations (tag `v1.0.0`, the Java original); supports `--smart/--lowercase/--no-stop-words` |
| `benches/ik_benchmark.rs` | Criterion benchmarks (run with `cargo bench`). |

```bash
cargo run --release --example perf_cmp benches/corpus.txt
cargo run --release --example judge_verify_stress -- --threads 4 --chunks "0,1,3,7,64,4096"
# Line-by-line comparison against v1.0.0
cat benches/corpus.txt | cargo run --release --example segment_dump -- --no-stop-words
```

---

## 20. Custom stop words

Matching tokens **do not appear in the final output** (they are filtered out, not emitted as
separate tokens). The source checks `is_stop_word` at the output stage and skips
(`analyze_context.rs`):

```rust
let factory = TokenizerFactory::new(
    Config::builder()
        .add_ext_stop_word_owned_text("的\n了".to_string())
        .build(),
);

let words: Vec<String> = factory.segmenter("我的书").map(|l| l.text().to_string()).collect();
println!("{:?}", words); // ["我", "书"]  — "的" was filtered out
```

> This matches the original IK Analyzer: "stop words" are an output-level filter, not an
> indexing-time "keep but do not index". If your business logic needs "keep and mark", use a
> custom dictionary instead of stop words.

---

## 21. FAQ

**Q: Why do I get a deprecation warning when using `IKSegmenter::new(text, true)` directly?**
A: That method rebuilds dictionaries on every call, which is expensive, and has been deprecated.
Use `TokenizerFactory::new(config).segmenter(text)` instead.

**Q: Does `segmenter(text)` reload dictionaries every time?**
A: No. `TokenizerFactory::new` loads and compiles them once into `Arc<Dictionary>`; afterwards
`segmenter()` only performs lightweight allocation.

**Q: Should I choose smart or fine?**
A: Use smart when you need clear semantics and concise results (display, keywords); use fine for
high recall and inverted indexes. Both match the original's segmentation quality.

**Q: Can multiple factories with different dictionaries run at the same time?**
A: Yes. `Arc<Dictionary>` isolates them, so multiple factories with different configurations can
coexist in one process (the original's global singleton cannot).

**Q: Are supplementary-plane rare characters (such as `𪚥`) recognized?**
A: Yes, and the behavior is configurable (`AstralMode`, see §6.5). The default `Ideographs`
recognizes CJK Extension B–I as ideographs (the original Java `char` is a UTF-16 code unit, so
anything above U+FFFF is split into surrogate halves and dropped); other supplementary-plane
characters such as emoji are dropped by default and can be emitted with `AstralMode::Tokens`;
`AstralMode::Drop` is the strict u6 regime (the value pinned by `Config::u6_parity`).

**Q: What units do `begin_position`/`end_position` use, and can they be compared with the Java original?**
A: The Rust core uses **code point (Unicode scalar) indices**; Python `cut` uses **byte
(UTF-8) offsets**; the Java original uses **UTF-16 code units**. The three are **not directly
comparable**: convert when comparing against Java (code points → UTF-16: BMP 1:1, astral 1→2).
The **token text and type sequences are unaffected**; the parity contract is defined on the
token sequence (see exception #4 in §6.4).

**Q: Are Python `cut` `start/end` character or byte offsets?**
A: **Byte (UTF-8) offsets**, so you can slice the original Python string directly.

**Q: How do I fall back to the arbitration implementation that is byte-identical to the original?**
A: `judge-fast` is the default (O(n), faster smart mode). To use `judge_reference`, build with
`--no-default-features`.

**Q: Does the CLI use the bundled IK main dictionary when `--custom-dict` is omitted?**
A: Yes. When no custom dictionary is given, the CLI loads the bundled default main dictionary.

---

> Source locations referenced in this document: `src/lib.rs` (public API), `src/cfg/mod.rs`
> (`Config`/`DictSource`), `src/core/ik_segmenter.rs` (factory and segmenter),
> `src/core/lexeme.rs` (`Lexeme`), `src/core/char_source.rs` (streaming),
> `python/src/lib.rs` (Python bindings), `src/bin/ik-analyzer.rs` (CLI).
