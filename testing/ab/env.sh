# shellcheck shell=bash
# A/B 对拍工装环境配置。
# 用法：source "$(dirname "$0")/env.sh"
#
# Java 侧源码树无默认值，需自行检出后用环境变量指定（见 README）。
# 本脚本只设置变量、创建生成目录，不做任何编译。

# ---- 工装根目录（即本文件所在目录）----
AB_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# ---- 生成目录（gitignore 内，不入库）----
AB_GEN="${AB_GEN:-$AB_ROOT/gen}"
mkdir -p "$AB_GEN"

# ---- 两侧 Java 源码树（需自行检出，无默认值）----
# A（基准）：IK Analyzer 2012_u6 干净原始版，指向其 src 目录
#   例：export AB_A_SRC=$HOME/src/IK-Analyzer-2012_u6_source/src
AB_A_SRC="${AB_A_SRC:-}"
# B（演进参考）：analysis-ik，指向 core/src/main/java
#   例：export AB_B_CORE=$HOME/src/analysis-ik/core/src/main/java
AB_B_CORE="${AB_B_CORE:-}"

# ---- B 侧编译依赖（离线 Maven 仓库）----
AB_M2="${AB_M2:-$HOME/.m2/repository}"

# 取某 artifact 的最新版本 jar（单个无换行的绝对路径）
_ab_m2jar() {
  find "$AB_M2/$1" -name '*.jar' ! -name '*sources*' ! -name '*javadoc*' 2>/dev/null \
    | sort | tail -1 | tr -d '\n'
}

# 依赖清单与 analysis-ik core/pom.xml 对齐：
#   lucene-core, lucene-analysis-common, httpclient, httpcore,
#   commons-logging, commons-codec, log4j-api
AB_B_CP="${AB_B_CP:-}"
if [ -z "$AB_B_CP" ]; then
  _items=(
    "org/apache/lucene/lucene-core"
    "org/apache/lucene/lucene-analysis-common"
    "org/apache/httpcomponents/httpclient"
    "org/apache/httpcomponents/httpcore"
    "commons-logging/commons-logging"
    "commons-codec/commons-codec"
    "org/apache/logging/log4j/log4j-api"
  )
  AB_B_CP=""
  for _gav in "${_items[@]}"; do
    _p="$(_ab_m2jar "$_gav")"
    [ -z "$AB_B_CP" ] && AB_B_CP="$_p" || AB_B_CP="$AB_B_CP:$_p"
  done
fi

# ---- 可选：额外的 405 行对账语料（不随仓库分发，按需指定）----
AB_CORPUS_LEGACY="${AB_CORPUS_LEGACY:-}"

# ---- B 侧词典目录（由 assemble_dicts.sh 在构建期从两侧源码树装配）----
# C1 默认：与 A 侧词典逐字节相同，用于隔离算法差异
AB_BDICT_UNIFIED="${AB_BDICT_UNIFIED:-$AB_GEN/probes/bcfg_unified}"

# ---- 编译输出 ----
AB_A_CLASSES="$AB_GEN/aclasses"
AB_B_CLASSES="$AB_GEN/bclasses"
AB_PROBE_CLASSES="$AB_GEN/probe_classes"

# ---- 两侧 classpath ----
# A 侧必须把 gen/probes/acfg 放在最前：它提供 C1 口径的 IKAnalyzer.cfg.xml
# （ext_dict=long_word_dict.dic / ext_stopwords=test_stopwords.dic），
# 否则会从 $AB_A_SRC 解析到 u6 自带 cfg.xml（ext_stopwords=stopword.dic），
# 导致 A 也加载 32 个英文停用词，掩盖报告中「内置停用词」这一根因。
# 该目录由 assemble_dicts.sh 在构建期装配。
AB_ACP="$AB_PROBE_CLASSES:$AB_GEN/probes/acfg:$AB_A_CLASSES:$AB_A_SRC"
AB_BCP="$AB_PROBE_CLASSES:$AB_B_CP:$AB_B_CLASSES"

# ---- 依赖自检 ----
ab_check() {
  local missing=0
  if [ -z "$AB_A_SRC" ] || [ -z "$AB_B_CORE" ]; then
    echo "[env] 请用 AB_A_SRC / AB_B_CORE 指定 u6 与 analysis-ik 源码树（见 README）" >&2
    missing=1
  fi
  for p in "$AB_A_SRC" "$AB_B_CORE"; do
    if [ -n "$p" ] && [ ! -d "$p" ]; then echo "[env] 缺失源码树: $p" >&2; missing=1; fi
  done
  if ! command -v javac >/dev/null; then echo "[env] 缺少 javac（需 JDK 21+）" >&2; missing=1; fi
  if ! command -v python3 >/dev/null; then echo "[env] 缺少 python3" >&2; missing=1; fi
  if [ "$missing" -ne 0 ]; then return 1; fi
  # 逐 jar 检查
  local jar
  while IFS= read -r jar; do
    [ -z "$jar" ] && continue
    if [ ! -f "$jar" ]; then echo "[env] 缺失依赖 jar: $jar" >&2; missing=1; fi
  done < <(tr ':' '\n' <<< "$AB_B_CP")
  [ "$missing" -eq 0 ]
}
