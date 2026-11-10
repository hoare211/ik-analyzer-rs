# u6 与 analysis-ik A/B 对拍工装

> 作用：把开发期两份差异调研报告的全部 A 级（实测）结论变成**可复算**的证据链
> （报告本身不随仓库分发）。
>
> 前置：需自行检出 IK Analyzer 2012_u6 与 analysis-ik 源码树，并按 `env.sh`
> 的说明用 `AB_A_SRC` / `AB_B_CORE` 指定路径。

## 原理

把两侧 Java 源码各自编译成可执行体，用相同词典、相同输入跑分词，逐 token 比对
`text[TYPE](begin-end)`。**词典相同时，任何输出差异都是算法差异；词典不同时，差异混入数据因素**——所以本工装提供三种对照配置（见 README §「三种对照配置」）。

## 目录结构

```
testing/ab/
├── README.md            本文件
├── env.sh               路径与依赖配置（可被环境变量覆盖）
├── build.sh             编译 A/B 两侧 + 5 个 Java 探针，并调用 assemble_dicts.sh
├── assemble_dicts.sh    从两侧源码树装配 B 侧词典目录（避免把 3MB 词典入库）
├── cmp.sh               跑一轮对拍（同词典）
├── run_all.sh           复现报告中全部 8 组实验
├── java/                探针源码
│   ├── AProbe.java          u6   token 探针   输出 text[TYPE](begin-end)
│   ├── BProbe.java          analysis-ik token 探针
│   ├── CharProbeA.java      u6   预处理探针    逐 code unit 输出 identifyCharType/regularize
│   ├── CharProbeB.java      analysis-ik 预处理探针
│   └── DecodeProbe.java     验证 Java UTF-8 解码器对畸形代理字节的行为
├── py/                  差分与归因脚本
│   ├── tokendiff.py            逐 token 集合差
│   ├── compactdiff.py          紧凑差异（首个分歧点）
│   ├── build_repertoire.py     生成 9500 code unit 字符表
│   ├── analyze_chars.py        预处理阶段差分分析
│   ├── build_char_corpus.py    生成 254 用例字符系统语料
│   ├── analyze_char_corpus.py  语料差异按类别/根因归因
│   ├── analyze_enc.py          编码阶段分析（跨页/覆盖率/重叠）
│   ├── build_probe_split.py    生成 125 位置代理对跨页扫描输入
│   ├── build_probe_enc.py      生成 astral × 缓冲区边界输入
│   ├── build_probe_long.py     生成 ≥4097 字符边界输入
│   └── final_summary.py        汇总根因归因
├── probes/              小型探针输入与 cfg.xml 模板（入库，共 260K）
│   ├── templates/           cfg.xml 模板 + 小型探针词典（dense/case/long_word/stopwords）
│   └── *.txt                报告逐条引用的定向探针输入
└── gen/                 生成产物（.gitignore 内，不入库）
    ├── aclasses/ bclasses/ probe_classes/
    └── probes/              装配出的 B 侧词典目录（含 3MB main2012.dic 拷贝）
```

> **为什么词典不入库**：C1 需要 B 侧有一份与 A 侧逐字节相同的 `main2012.dic`
> （3MB）。入库三份拷贝约 9MB 且与源码树重复，故 `assemble_dicts.sh` 在构建期
> 从两侧源码树装配到 `gen/probes/`，并当场 `cmp` 校验逐字节一致。

## 前置条件

| 项 | 要求 | 说明 |
|---|---|---|
| JDK | **21+**（本仓库实证环境 OpenJDK 21.0.4） | B 侧用了 `Character.UnicodeBlock` 的新版块 |
| Maven 本地仓库 | `~/.m2/repository` 内含 7 个依赖 jar | 见下表；全部可离线解析，**无需 maven 可执行文件** |
| A 源码树 | `~/projects/ik_dev/refs/IK-Analyzer-2012_u6_source/src` | **必须用 `refs/` 干净原始版**，不要用 `ik-repos/` 的埋点版 A′，否则行号偏移 |
| B 源码树 | `~/projects/ik-repos/analysis-ik/core/src/main/java` | `master @ 6d2d70f` |

B 侧依赖（与 `analysis-ik/core/pom.xml` 对齐，`env.sh` 自动定位最新版本）：

| groupId:artifactId | 实证版本 |
|---|---|
| `org.apache.lucene:lucene-core` | 9.0.0 |
| `org.apache.lucene:lucene-analysis-common` | 9.0.0 |
| `org.apache.httpcomponents:httpclient` | 4.5.13 |
| `org.apache.httpcomponents:httpcore` | 4.4.15 |
| `commons-logging:commons-logging` | 1.2 |
| `commons-codec:commons-codec` | 1.16.1 |
| `org.apache.logging.log4j:log4j-api` | 2.21.1 |

路径不符合默认值时用环境变量覆盖：

```bash
export AB_A_SRC=/path/to/u6/src
export AB_B_CORE=/path/to/analysis-ik/core/src/main/java
export AB_M2=/path/to/m2
```

## 运行

```bash
cd testing/ab
./build.sh        # 一次性编译 + 装配词典，产物在 gen/
./run_all.sh      # 复现报告中全部 8 组实验
# 或单跑某一组
./cmp.sh probes/emoji.txt maxword
```

## 三种对照配置

### C1 同词典 · 工装口径（隔离**算法**差异）

A 侧 classpath 把 `gen/probes/acfg/`（含 u6 工装同款 `IKAnalyzer.cfg.xml`）放在最前，
再接 `refs/src`（供 `main2012.dic`/`quantifier.dic`）；B 侧 `confDir` 指向
`gen/probes/bcfg_unified/`（内容与 A 逐字节相同）。**这是 `cmp.sh` 的默认行为。**

### C2 各自默认分发（量化**换词典**的总量影响）

```bash
./cmp.sh "$AB_CORPUS_LEGACY" maxword lower "$HOME/projects/ik-repos/analysis-ik/config"
# A 侧改用 u6 自带 cfg.xml：
java -cp "gen/aclasses:$AB_A_SRC" AProbe maxword < "$AB_CORPUS_LEGACY"
```

### C3 定向探针（隔离**单条**差异）

见 `probes/*.txt`，每个探针对应报告中一条已归因的差异。
C3 需替换词典时通过 `run_cmp` 的第 3/4 个参数（B 词典目录 / A 侧额外 classpath 前缀）传入，
A 侧额外前缀必须放在 classpath **最前**，否则 u6 自带 cfg.xml 会抢先被
`getResourceAsStream` 命中（README §已知注意事项 6）。

## 预期结果（实证基准，重新运行应逐字节一致）

| 实验 | 预期 |
|---|---|
| 预处理 9500 code unit | `identifyCharType` 差异 = 2048（全部代理区）；`regularize`(low=true) 差异 = 0；(low=false) = 52 |
| 语料 254 用例 `max_word` | 相同 195/254；差异 59 例 = 代理对 33 + 词典 13 + 停用词 13 |
| 语料 254 用例 `smart` | 相同 199/254 |
| 代理对跨页扫描 125 位置 × 2 口径 | 拆分代理对 A=0 **B=4**；refill 重叠 A=22（每口径 11）B=0；未覆盖 unit A=502 B=4 |
| ≥4097 字符边界 | 输入 ≥4097 字符起 u6 每个 refill 边界多 1 个重叠 token |
| 孤立代理解码 | `\xED\xA0\x80` → 1 个 U+FFFD（3 字节 → 1 code unit） |

> **B=4 是本次迁库复算时新发现的缺陷**：当 astral 字符的高位代理恰好落在
> 第 4096 个 UTF-16 unit（缓冲区末格）时，B 的 `SurrogatePairSegmenter` 把该字符
> 拆成半个——高位代理单独输出为 `TYPE_CNWORD`（文本经 UTF-8 编码为 `?`），
> 低位代理被丢弃，偏移空间留下 1 个 unit 的空洞。触发点在扫描输入的 p=4095 两行。
> 复算方式见本目录 `run_all.sh` 的实验 3。

### 检测陷阱（为什么这两个指标曾得出错误结论）

1. **不能用 token 文本判定代理对拆分**：孤立的高位代理经 stdout 以 UTF-8 编码会变成
   `?`（无法编码），按文本判定会漏检。必须回查输入原文在该偏移上的码点。
2. **不能用「最后一个 token 的 end」判断覆盖完整性**：max_word 输出顺序非按位置排序
   （嵌套词元在后），last_end 与总 unit 数不等是正常现象。必须用**覆盖率**。
3. **类型不能限定 CNCHAR**：B 的 `outputSingleCharLexeme` 用的是 `TYPE_CNWORD`
   （`SurrogatePairSegmenter.java:88`）。

## 已知注意事项

1. **u6 的 `Dictionary` 会往 stdout 打「加载扩展词典：…」**。`cmp.sh` 已用
   `grep -E '^#'` 过滤；手写 Java 调用时必须做同样处理，否则输出整体错位一行。
2. **B 侧必须显式 `Dictionary.initial(...)`**。脱离 ES/OpenSearch 使用其 core 时，
   `getSingleton()` 会抛 `IllegalStateException`。`BProbe.main` 已处理。
3. **B 的 `main.dic` 带 UTF-8 BOM**。C1 统一改用 u6 的无 BOM `main2012.dic` 作为
   `main.dic`，避免 BOM 变量污染算法对比。
4. **B 缺 `surname/suffix/preposition.dic` 会启动失败**（`critical=true`）。
   `assemble_dicts.sh` 装配时已备占位文件（用 u6 量词词典充数，B 中加载即丢弃）。
5. **报告引用的行号一律取自 `refs/` 干净原始版（A）**；`ik-repos/` 的 A′ 仅用于
   编译运行，不可用于行号取证。
6. **A 侧 cfg.xml 的 classpath 顺序敏感**：`getResourceAsStream` 取第一个命中。
   C3 替换词典时必须把对应目录放在 classpath **最前**，否则 u6 自带
   `IKAnalyzer.cfg.xml`（`ext_dict=ext.dic`）会抢先命中，探针词典形同虚设——
   本次迁库就因此把实验 6 的 `ABC公司` 词条静默失效了，靠对照报告基准数字才发现。
