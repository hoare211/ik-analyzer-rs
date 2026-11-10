# Changelog

本文件记录 ik-analyzer-rs 的发布变更。格式参考 [Keep a Changelog](https://keepachangelog.com/)。

> **定位说明**：1.2.0 及更早的条目记录的是当时以「IK 2012_u6 的 1:1 复刻」为目标的阶段，
> 按当时语境原样保留。自 1.3.0 起定位已调整为：以 analysis-ik 为实现基准的独立 Rust 实现，
> 默认输出是产品决策，与 u6 的行为一致由 `Config::u6_parity` 显式、按需开启。

## [1.3.0] - 2026-10-12

> 本版本为项目**首次公开发布**，公开仓库的历史从本版本开始。

**目标重新对齐**：实现层以 analysis-ik（`master@6d2d70f`）为基准，默认输出作为独立的产品决策
（可优于 u6）；与 IK 2012_u6 的行为一致性改由 `Config::u6_parity` **显式、按需开启**的契约保证，
而非项目整体定位 —— 本项目不是 u6 的逐字移植。

### Added

- **`Config::u6_parity(use_smart)`**：IK 2012_u6 行为 parity 预设。钉住冻结的
  u6 口径配置——u6 分发词典（`main2012.dic` + `ext.dic` + `quantifier.dic`，
  经 `u6_dict_sources()` 直接引用，未来默认词表扩容不破坏它）+ 停用词置空 +
  `enable_lowercase=true` + `AstralMode::Drop`；命中独立的 `DictSet::U6`
  缓存键（与默认词表分立）。Python 绑定同步提供
  `ik.Tokenizer.u6_parity(use_smart=True)`。
- **`AstralMode` 三值枚举**（`Drop` / `Ideographs` / `Tokens`，默认
  `Ideographs`）与 `ConfigBuilder::astral_mode` / `set_astral_mode`：
  补充平面（astral）字符处理从硬编码变为显式配置。`Drop` = u6 严格口径；
  `Ideographs` = CJK 扩展 B–I 识别为汉字、emoji 丢弃（默认，产品最优）；
  `Tokens` = 全部非 BMP 输出为单字 `CNCHAR` 词元（机制对齐 analysis-ik 的
  `SurrogatePairSegmenter` 目标，但不照搬其两个已实证缺陷）。Python 绑定
  暴露 `ik.AstralMode` 与 `Config.astral_mode(...)`。
- **词典 BOM 剥离**：词表行清洗剥离前导 UTF-8 BOM（`U+FEFF`），覆盖内置文本
  源 / `File` 源 / `add_ext_*` 用户文件（u6 不剥离会污染首词条，
  analysis-ik `Dictionary.java:199-200` 显式剥离）。对格式正确的词表零行为变更。
- 新增集成测试：9 个含 ASCII 词条端到端、全角大写 else-if、`AstralMode`
  三模式、astral 跨 4096 换页完整性、`u6_parity` 预设双模式；新增 lib 测试：
  BOM 剥离（文本 + 文件源）、U6 计划缓存分立。

### Fixed

- **CJK 子分词器门控**（移植期 Bug，v1.0.0 即存在）：`!= CHAR_CHINESE` →
  **`== CHAR_USELESS` 才跳过**，与 u6 `CJKSegmenter.java:54` / analysis-ik
  同位置逐字一致。修复后 9 个内置词条（`A股`/`B股`/`AB股`/`H股`/`K线`/`U盘`/
  `QQ宠物`/`QQ飞车`/`Hold住`）与**全部**用户混合词条端到端可达（此前词典层
  测试是通的、端到端被门控挡死，属测试盲区）。见
  《问题记录-大小写全角与词典匹配差异.md》§1。
- **全角大写 `regularize` 顺序**（v1.0.2/v1.1.1 共享）：顺序式（全角→半角后
  叠加小写，`Ａ`→`a`）→ **else-if 链**（`Ａ`→`A`，不叠加小写），与 u6
  `CharacterUtil.java:89-101` / analysis-ik `:94-103` 逐字同构。连带修正
  `test_default_fullwidth_halfwidth_equivalent` 固化的错误等价（u6 中
  `ＡＢＣ`→`ABC` 与 `ABC`→`abc` 输出本就不同）。见问题记录 §2。

### Changed

- **README / 使用手册口径**：三层定位声明（实现基准 analysis-ik / 行为 parity
  显式契约 / 默认值产品决策）；新增使用手册 §6.4「与 IK 2012_u6 的行为 parity
  契约」（配置映射表 + 例外清单）与 §6.5「`AstralMode`」；例外清单中
  `cursor+1` 与 BOM 剥离由「未披露例外」改列「**u6 缺陷修复**」，`sat_mul`
  改列「纯稳健性改进」。
- 核心源码注释锚点统一为「analysis-ik 行号为主、u6 差异处标注」，补齐此前
  无对齐声明的文件头（`cjk_segmenter.rs` / `character_util.rs` /
  `analyze_context.rs` / `cn_quantifier_segmenter.rs` / `letter_segmenter.rs` /
  `dictionary.rs` 等）。

### 复审收尾（2026-10-11）

**2026-10-11 复审的收尾项**。

#### 修复

- **词典行空白裁剪集过宽导致「XY村」村名切分与 u6 不一致**
  （缺陷分析与复现步骤见仓库 Issue）：
  `clean_dict_word` 曾用 `str::trim()`（Unicode `White_Space` 全集），而 u6 的
  `String.trim()` 只裁码点 ≤ U+0020。geocoding region.dic:249714 的词条是
  `"\u{3000}村"`（U+3000 前缀污染行）——Rust 把它裁成 `"村"`，`村` 因此进入仲裁
  候选集，`深潭村` 被切 `[深潭, 村]`，而 u6（`村` 不成词）切 `[深, 潭村]`。
  修复：裁剪集收紧为「≤U+0020 + U+00A0（NBSP）」——前者与 u6 逐字一致，
  后者是 §7.4 声明保留的有意改进（`方正凤凰城\u{a0}{4}` 类尾附 NBSP 词条
  u6 不可命中、本库可命中）。U+3000 等其余 Unicode 空白改为**不裁**（u6 语义）。
  验证：10000 行真实地址语料对 Java u6 直驱 jar（ikanalyzer-2012_u6.jar +
  相同扩展词典）全量对拍，分歧行 **25 → 1**（残余 1 行即上述 NBSP 差异）；
  issue 所列 22 个村名逐一对拍 0 分歧。
- **N-1** Python 绑定残留 3 处 `builder.clone()`：1.2.0 消除了 12 个 `with_*`/`add_ext_*`
  方法中的 9 处，**遗漏的 3 处**在本轮补齐，改用已存在的 `set_*` 变体
  （`ConfigBuilder` 原地修改，不再深拷贝含 `Cow::Owned` 大文本扩展词典的全部词源）：
  - `with_main_dict_from_sources` → `set_with_main_dict`
  - `with_quantifier_from_paths` → `set_with_quantifier`
  - `with_stop_words_from_paths` → `set_with_stop_words`
- 绑定层 `segment` / `cut` / `segment_batch` 的三处 `Ok(PyList::new(py, ..)?)` 修正为
  直接返回（语义等价，消除 `clippy::needless_question_mark`）。

#### 工程化

- **CI clippy 覆盖缺口**：本 workspace 含根 package，cargo 默认只选根 crate，
  故 `cargo clippy --all-targets` 从不编译 `python/`（cdylib，CI 也无其它 job 编译它），
  绑定层 lint 长期静默失效——上面三处 `needless_question_mark` 正是由此漏网。
  clippy job 改为 `cargo clippy --workspace --all-targets`。

#### 文档

- **N-2** 阶段性文档仍写「CJK 扩展 B–G」（主文档已是 B–I），已加注说明范围扩展至
  **B–I** 并以主文档为准。

## [1.2.0] - 2026-10-07

**2026-10-07 代码评审修复**。
本轮是对上一轮（1.1.2）落地结果的复审，共修复 2 个 P0 / 3 个 P1 / 3 个 P2 /
6 个 P3 / 10 个 P4，并**撤回 1 个误判**（见「撤回」一节）。

### 修复

- **P0-1（严重）换页后静默丢字符**：`analyze_context::output_to_result` 曾把**全局**
  坐标 `Lexeme::end_position()`（= `offset + begin + length`）赋给**页内**下标 `index`。
  第 1 页 `offset == 0` 时两坐标系恰好相等故缺陷完全隐匿；第 2 页起 `index` 一步跳出
  页内 `cursor`，`while` 循环立即退出 → **该页剩余字符被静默丢弃**（实测恒丢末尾 2 字，
  阈值 `输入长度 >= 4101`，内存与流式路径均复现）。修复为页内坐标 `begin + length`。
  该缺陷由 1.1.2 调整 `mark_buffer_offset`/`fill_buffer` 的 `cursor+1` 语义时引入。
- **P0-2 流式 IO 错误不可观测**：读取失败时 `Iterator` 无法返回错误，原实现只落
  `log::error!`，而 `env_logger` 已是 optional feature → 下游默认无输出，表现为
  「正常结束但内容被截断」。新增 `IKSegmenter::take_error()` / `has_error()`，
  并在 `segmenter_from_read` 文档中说明截断行为。
- **P1-2 页内下标缺契约**：`add_lexeme_path` 把可能为 `-1` 的 `path_begin` 直接
  `as usize` 当下标，补 `debug_assert` 显式校验范围。
- **P1-3 `Lexeme::new` 信任边界**：补充文档说明非受信入参（负 `length`）的后果与校验建议。
- **P4-1~P4-5、P4-10 代码卫生**：删除残留演示注释与被注释掉的旧实现、清理 3 个死常量
  `SEGMENTER_NAME`、`core/mod.rs` 与 `dic/mod.rs` 的模块写法统一、
  `StrCharSource` 的「恒定内存」措辞限定到 read 路径。

### 性能

- **P2-2 `judge_fast` 常驻 scratch 复用**：原先每次调用新建 15 个 `Vec`（约 15 次堆分配），
  而其自比照的 `judge_reference` 只 2 次（复用 scratch）。`IKArbitrator` 改为带字段结构体，
  `process`/`judge*` 改 `&mut self`，15 个临时数组全部跨调用复用。
  实测分配次数 −5,117、**分配字节 −34%**（18.71 → 11.79 MiB），耗时持平。
- **P2-1 零拷贝入口 `segment_spans()`**（新增 API）：产出 `Copy` 的 `Span`（位置 + 类型），
  **不做每词元一次的 `String` 文本分配** —— 该分配实测占分词热路径全部堆分配的 **94%**。
  配套 `char_to_byte_map()` 辅助函数。
  实测分配次数 **−98.6%**（205,307 → 2,959）、耗时 −5.5%。
  文档明确 `Span::slice` 返回**原文**而 `Lexeme::text()` 返回**归一化后**文本
  （位置与类型一致）。
- **P2-3 中文数词表**：`CHN_NUMBER_CHARS` 由 `LazyLock<Vec<char>>` 改为真正的
  `&'static [char]` 排序字面量（每字符省一次 lazy 原子读 + 堆间接寻址）；
  修正「35 个元素」的事实错误（实为 34 字 / 33 唯一）。

### 新增

- `IKSegmenter::take_error()` / `has_error()`：流式 IO 错误的精确判定出口。
- `Span` / `SpanSegmenter` / `TokenizerFactory::segment_spans()` / `char_to_byte_map()`：
  零拷贝词元区间入口。
- `ik_analyzer_rs::VERSION` 常量；Python 侧 `__binding_version__`。
- `ConfigBuilder` 的 15 个 `&mut self` 变体（`set_with_*` / `set_add_ext_*`），
  消除 Python 绑定层每次 setter 的深拷贝（曾含大文本 `Cow::Owned` 扩展词典）。
- `examples/alloc_count.rs`：常驻分配计量工具（`#[global_allocator]` 置于 example 内，
  **无需 feature gate**），输出耗时/分配次数/分配字节及 `segmenter` vs `segment_spans` 对照。
- 测试：换页参数化扫描 + 位置覆盖式不变式断言、流式 IO 错误用例、停用词整段语义护栏、
  `segment_spans` 等价性与切片正确性、`char_to_byte_map` 形状自洽、版本一致性、
  中文数词表。集成测试 21 → 34，lib 29 → 32，doctest 2 → 7。

### 变更

- **P3-1 版本号单一源**：此前根 crate 1.1.2 而 `python/Cargo.toml` /
  `python/pyproject.toml` 停在 1.1.1，导致 Python 侧 `__version__` 报旧版本，
  且使用手册把 `'1.1.1'` 写进示例输出把 bug 固化为预期行为。现统一由
  `[workspace.package]` 声明，各 crate 以 `version.workspace = true` 继承；
  `__version__` 改为取核心库版本。
  注意：`rust-version` 必须**显式** `rust-version.workspace = true` 继承到每个 crate，
  否则 clippy 失去 MSRV 门禁会重新触发新版 lint。
- **P4-7** `ConfigBuilder` 新增 15 个 `set_*`（`&mut self`）变体，Python 绑定的 12 个
  `with_*`/`add_ext_*` 方法中 9 处已改用它消除 `builder.clone()`。**剩余 3 处当时遗漏**
  （`with_main_dict_from_sources`、`with_quantifier_from_paths`、`with_stop_words_from_paths`），
  已在 [未发布] 一节补齐。
- **P4-8** `PyLexeme.start/end` 为 **byte 偏移**，已在 Rust doc 与 `.pyi` 三处写明
  （此前无任何提示，Python 用户几乎必然按 code point 理解）。
- **P4-6** 绑定层注释误引 Rust CLI 的 `--custom-dict` flag，已更正。

### 文档

- **P3-2** 使用手册仍在文档化 1.1.2 已删除的 `probe-count`/`probe-mimalloc`，
  其命令现会直接 cargo 报错。改指向常驻工具 `examples/alloc_count.rs`，
  并记录「被删 feature 未给替代物」这一教训。feature 表补 `env_logger`。
- **P3-3** 生僻字支持范围 B–G → **B–I**（Unicode 15.1），代码与文档此前不一致。
- **P3-4** README 输出示例含 `"的"`（默认停用词会被滤除），已修正。
- **P3-5** 修正 7 处评审编号错挂/不存在（`P1-15`、`P0-2.3`、`P0-2`、`P1-8`、
  `评审 P2 type_complexity`、`评审 P0/P1`、`评审 P0`），统一到可追溯的具体 ID。
- **P3-6** 撤回「~21ms 反超原版文档值 ~31ms」的性能论断：原版对照环境已不可复现，
  该数字不构成同机对比结论，降级为量级参考。
- 新增手册章节：§7.1 零拷贝入口 `segment_spans`、§9.1 流式读取失败的判定。

### 工程化

- **P4-9 CI 增强**：`no-default-features`/`msrv` job 补测试（原先只 build）；
  新增 `doc`（rustdoc intra-doc link）、`package`（断言 `dict/*.dic` 已打进包）、
  `versions-consistent`（三处版本一致）三个 job。

### 撤回

- **P1-1（评审误判，已撤回）**：评审报告曾断言 `CompactTrie::match_text` 的「整段成词」
  语义偏离 IK 原版（原版被记为逐位置推进）。该结论来自对原版源码的记忆而非项目内实证，
  **实施后分词输出立即回归**（smart 202231→202215、fine 161328→159815）。
  根因：`Dictionary::is_stop_word` 的调用点传入**词元长度**（`length > 1`），
  改成前缀语义会使任何「前缀命中停用词」的长词元被静默误删
  （`停用词 {"中"}` + `主词典 {"中国"}` → `"中国"` 整体消失）。
  推翻依据：与项目已完成的 u6 对齐结果矛盾（9133 行语料 × 4 口径对 u6 三方差分 0 分歧）。
  **「整段成词」是与停用词过滤配套、已验证的既定正确行为，非缺陷。**
  现已固化为护栏测试 + 文档，防止再次被误「优化」。

## [1.1.2] - 2026-10-01

**代码评审 P0–P4 修复落地**，
并完成仓库 git 治理。

### 修复
- P0 正确性/健壮性 8 项：`IkError` 路径丢失与 TOCTOU、词典非法 UTF-8 行静默丢弃、
  Python panic 版工厂、`CompactTrie` 公开 API 收敛、`try_decode` 递归改迭代并感知
  IO 错误、`Lexeme::hash` 整数溢出、`LexemePath::p_weight` 溢出、差分测试空转。
- P1 文档/注释与实现一致性（含"无虚分发"等不实措辞）、性能常数因子
  （`HashSet<char>` 热路径负优化等）、依赖精简（移除 `once_cell`、`env_logger`
  转 optional、删除死 feature `probe-count`/`probe-mimalloc`）、Unicode 区间补全。

### 新增
- CI（`.github/workflows/ci.yml`）与 rustfmt 配置；`Cargo.lock` 纳入版本管理。
- `examples/dump_old.rs`：旧版分词对照（bisect）工具收录。

### 变更（仓库治理）
- 主线统一为 `main`（`master` 快进后改名）；删除已并入/已被重写覆盖的分支 8 个，
  未并入实验以 `archive/*` tag 归档。
- 版本 tag 收敛到主线：旧 `v1.0.0`/`v1.0.1`/`v1.0.2` 重命名 `*-legacy`；
  主线重打 `v1.0.0`（原始基线 5f27a0c）与 `v1.1.0`。

## [1.1.1] - 2026-09-26

**默认配置回归 v1.0.0（IK Analyzer 2012_u6）行为**，并补齐大小写开关在 Python / CLI
侧的暴露。语义对账见下（唯一有意偏差 = 停用词表）。

### 修复
- **`use_smart` 默认值恢复为 `true`**：`ConfigBuilder::new()` 自 `4ac8289`（「使用预编译
  DAWG 替代运行期 Trie 构建」，提交信息与默认值无关）起被误改为 `false`，而 CHANGELOG
  1.1.0 仍声称「默认分词行为与 1.0.0 一致」、README/使用手册均写「默认 smart=True」。
  影响面：`Config::builder().build()` 与 Python `ik.Tokenizer()` 的默认模式由细粒度
  回到 **smart**（tag `v1.0.0` 的 `ConfigBuilder::new()` 默认值亦为 `true`）。
- 文档修正：CLI 示例输出（`的` 已被停用词过滤）、`add_ext_dict_paths` 签名
  （`Vec<PathBuf>`）、Python `__version__`。

### 新增
- Python 绑定补 `Config.enable_lowercase(bool)`：此前 Python 侧无法配置大小写，与 Rust
  侧能力不对等。
- CLI 新增 `--lowercase true|false`（默认 `true`，同 v1.0.0）。
- `examples/segment_dump.rs`：逐行分词对账工具（stdin → stdout，每行 `/` 连接词元），
  供与 tag `v1.0.0` / Java 原版做 `diff` 差分。
- 集成测试锁定默认语义：`test_default_config_equals_v1_0_0_profile`、
  `test_default_use_smart_is_true`、`test_default_case_insensitive`、
  `test_default_fullwidth_halfwidth_equivalent`、
  `test_default_dict_and_input_same_case_folding`。
- 使用手册新增 §6.4「与 v1.0.0（IK 2012_u6）的行为等价性」（等价矩阵 + 对账方法）。

### 与 tag v1.0.0 的对账结论
- `benches/corpus.txt` 9133 行（tag `v1.0.0` worktree 对照）：**fine 模式 0 分歧**；
  **smart 模式仅 1 行分歧**（第 5128 行）——v1.0.0 的 `AnalyzeContext.compound` 误写
  `if/else if`，漏掉「ARABIC → CN_NUM → 数量词」链式合并（`42三` + `首`）；1.1.0 已改为
  两个独立 `if`，与 2012_u6 **及** analysis-ik 5.0 Java 逐字一致。
- 当前默认（停用词启用）下与 v1.0.0 有 5634/9133 行输出不同，但差异 **100% 可由「停用词
  token 被过滤」解释**（仅上述 1 行例外），无其它语义漂移。
- **唯一有意偏差**：默认停用词表非空（v1.0.0 的 `dict/ext_stop_word/stop.dic` 为 0 字节，
  功能未生效）。需要严格逐字对齐时：`Config::builder().with_stop_words(Vec::new())`，
  或 `segment_dump --no-stop-words`。
- 词典文件 `main2012.dic` / `ext.dic` / `quantifier.dic` 与 v1.0.0 **md5 相同**；输入侧
  归一（全角→半角 + ASCII 小写）代码同源；词典侧由 v1.0.0 的 Unicode `.to_lowercase()`
  收敛为 ASCII 折叠（默认词表上不可观测，理由见使用手册 §6.4）。

### 兼容性
- 无 API 破坏；仅 `Config::builder().build()` 的**默认模式**由细粒度回到 smart
  （恢复 v1.0.0 与既有文档的语义）。Python 包版本 `1.0.0` → `1.1.1`，与 crate 同步，
  避免与 git tag `v1.0.0` 混淆。

## [1.1.0] - 2026-08-22

词典机制重构：**文本词典为唯一真源 + 运行期 DAWG 构建**（方案 C'）。

### 新增
- **`enable_lowercase` 配置开关**（默认 `true` = 2012_u6 原版小写化行为，吸收自
  analysis-ik 5.0 的 `enableLowercase`）：大小写策略在 **DAWG 构建期**生效——
  词典词与输入均仅做全角→半角 + ASCII 小写化；`false` 为严格精确匹配语义
  （`A股` ≠ `a股`，此时停用词表需自行收录大小写变体）。大小写变体词在构建期
  自然合并，无查询期折叠匹配。
- **运行期 DAWG 构建**（`src/dic/dawg_builder.rs`）：内置词典经 `include_str!`
  内嵌，首次使用时从文本构建紧凑 DAWG；词数/节点/边与旧预编译产物完全一致
  （规模锚点测试固化）。
- **默认词典全局缓存**：按（词典种类 × 大小写策略）进程内共享，同进程多个
  `TokenizerFactory` 复用同一份 DAWG（热命中 ~1.6µs）。
- 特性测试：构建期严格语义、变体词合并、停用词大小写逃逸（A 语义固有代价，
  与 analysis-ik 5.0 一致）。

### 变更
- **删除 build.rs 与 `dict_compiled/*.dawg` 预编译产物**：无预编译步骤，
  词典文本即全部事实源，下游 `cargo build` 不触发任何词典处理。
- **单后端化**：删除 `cfg::Dict` 枚举（Precompiled/Runtime 双后端）与
  `src/dic/trie.rs`，统一为 DAWG + `CompactTrie` 查询路径。
- `CompactTrie` 改持有 `Arc<[u8]>`（原 `&'static [u8]`）。
- `examples/measure_load.rs` 重写为「冷/热构建耗时 + RSS」测量工具。

### 性能（release，本机实测）
- 冷构建（默认三词典，一次性）：~62–75ms；热（缓存命中）：~1.6µs；
  构建期峰值 RSS ~52MB（完成后活内存 ~4MB；`/usr/bin/time -v` 所见 RSS
  含分配器保持的已释放页）。
- 基准交替对照基线 `aa5d082`：等价语义（`enable_lowercase=false`）下
  E2E Smart/Fine 与 Core Match 均持平——运行期 DAWG 零查询回归；
  默认 `true` 的 E2E 固有代价 ~6%（恢复小写化语义所致，token 数不同）。

### 兼容性
- **对外 API 无破坏**（`cfg::Dict` / `trie::Trie` 均非导出 API，仓库内外零引用）；
  默认分词行为与 1.0.0 一致（小写化）。版本号升 1.1.0 因新增 `enable_lowercase`
  能力与词典机制变更。

## [1.0.0] - 2026-08-19

首个可发布版本：IK Analyzer 2012_u6 的 Rust 1:1 忠实复刻。

### 核心能力
- **忠实复刻**：三子分词器（字母 / 量词 / 中文）+ 歧义裁决 + Trie 词典匹配 + 双缓冲流式处理，算法结构与词元语义（hash/ord 公式、类型体系、两种模式）与原版逐字对账一致。
- **两种模式**：`smart`（歧义消解）/ `fine`（细粒度），由 `ConfigBuilder::use_smart` 控制。
- **`TokenizerFactory` 工厂模型**：一次性加载词典并编译进 `Arc<Dictionary>`，之后 `segmenter()` 廉价创建分词器；支持 `segmenter_from_read` 流式恒定内存分词。
- **`ConfigBuilder` + `DictSource`**：支持文件与内存字符串词典来源；`with_*`（替换）与 `add_ext_*`（追加）两套语义。
- **`Lexeme` 输出**：含文本、起止位置、词元类型常量（与原版对应）。
- **多入口**：Rust crate、`pyo3` Python 绑定（`ik-analyzer-py`，提供 `segment`/`cut`/`segment_batch`）、内置 CLI。

### 针对原版的改进
- **生僻字全 Unicode 平面支持**（CJK 扩展 B–G），原版因 Java `char` 会丢弃 >U+FFFF 字符。
- **性能**：裁决 O(n²) → O(n)（`judge_fast`，默认 feature），smart ~21ms 反超原版 ~31ms，fine 对等；零 `unsafe`、无 `dyn` 虚分发、零分配回溯。
- **线程模型**：无全局单例，`Arc<Dictionary>` 支持多词典/多配置同进程共存、多线程共享。
- **类型化错误**：`try_new` 返回 `IkResult`，区分 `Io` / `FileNotFound` / `InvalidEncoding`。
- **流式鲁棒性**：增量 UTF-8 解码，正确拼接跨缓冲区多字节字符，非法 UTF-8 → U+FFFD。
- **配置便捷**：纯文本 `.dic` 可编辑词典 + 内存注入，告别 classpath / 二进制 `.dcm`。
- **依赖精简**：移除 `textwrap` / `terminal_size` / `unicode-width` / `dirs` 等重依赖。

### 正确性背书
- 13 组 MD5 级集成测试（含中英文混合、长词、全停用词、补充平面生僻字、跨缓冲区边界、词跨换页）。
- 4×10⁶ 用例重压差分（零分歧）。
- 大 cross_path 专项（词元数 ~200..2000、2×10⁴ 用例，覆盖 (300, 1976] 形态）。
- 多语料 judge-verify 双跑 + Python 绑定产出与 Rust 真值对账一致。

### 发布前收尾
- 默认 `judge-fast` 构建零警告（清理 `remove_tail`/`clone_from_path`/`poll_last`/`clone_from` 死代码 + 删除零调用 `peer_lexeme`）。
- 显式复跑 4×10⁶ 重压差分作为发布前正确性闸门（通过）。
- 发布前清理残留的 `maturin` 网络故障（wasi crate 下载），本地 `.so` 已验证可 import 并分词。

### 文档
- `docs/使用手册.md`：全场景使用手册。
