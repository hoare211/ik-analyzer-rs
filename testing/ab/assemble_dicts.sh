#!/usr/bin/env bash
# 装配 B 侧词典目录（gen/probes/bcfg_*）。
#
# 设计说明：B 侧需要一份与 A 侧逐字节相同的词典才能隔离算法差异（C1），
# 但 main2012.dic 有 3MB，不应入库。因此本脚本在构建期从两侧源码树装配，
# 仓库只保留 cfg.xml 模板与小型探针词典。
#
# 注意：B 的 loadSurnameDict/loadSuffixDict/loadPrepDict 用 critical=true，
# 缺文件会抛 RuntimeException 导致启动失败，故必须提供占位文件
# （用 u6 量词词典充数——它们在 B 中加载即丢弃，不参与查询）。
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=env.sh
source "$HERE/env.sh"

A_DIC="$AB_A_SRC/org/wltea/analyzer/dic"
TPL="$HERE/probes/templates"
OUT="$AB_GEN/probes"
mkdir -p "$OUT"

echo "== 装配 gen/probes/bcfg_unified（C1：与 A 侧词典逐字节相同）=="
mkdir -p "$OUT/bcfg_unified"
cp "$A_DIC/main2012.dic"                 "$OUT/bcfg_unified/main.dic"
cp "$A_DIC/quantifier.dic"               "$OUT/bcfg_unified/quantifier.dic"
cp "$AB_A_SRC/stopword.dic"              "$OUT/bcfg_unified/stopword.dic"
for f in surname suffix preposition; do
  cp "$A_DIC/quantifier.dic"             "$OUT/bcfg_unified/$f.dic"
done
cp "$TPL/long_word_dict.dic" "$TPL/test_stopwords.dic" "$OUT/bcfg_unified/"
cp "$TPL/cfg_unified.xml"    "$OUT/bcfg_unified/IKAnalyzer.cfg.xml"

echo "== 装配 gen/probes/bcfg_case（C3：ext 词典含 'ABC公司'）=="
mkdir -p "$OUT/bcfg_case"
cp "$A_DIC/main2012.dic"                 "$OUT/bcfg_case/main.dic"
cp "$A_DIC/quantifier.dic"               "$OUT/bcfg_case/quantifier.dic"
cp "$AB_A_SRC/stopword.dic"              "$OUT/bcfg_case/stopword.dic"
for f in surname suffix preposition; do
  cp "$A_DIC/quantifier.dic"             "$OUT/bcfg_case/$f.dic"
done
cp "$TPL/case_dict.dic" "$OUT/bcfg_case/"
cp "$TPL/cfg_case.xml"  "$OUT/bcfg_case/IKAnalyzer.cfg.xml"

echo "== 装配 gen/probes/bcfg_dense（C3：高密度兜底触发）=="
mkdir -p "$OUT/bcfg_dense"
cp "$A_DIC/main2012.dic"                 "$OUT/bcfg_dense/main.dic"
cp "$A_DIC/quantifier.dic"               "$OUT/bcfg_dense/quantifier.dic"
cp "$AB_A_SRC/stopword.dic"              "$OUT/bcfg_dense/stopword.dic"
for f in surname suffix preposition; do
  cp "$A_DIC/quantifier.dic"             "$OUT/bcfg_dense/$f.dic"
done
cp "$TPL/dense.dic" "$OUT/bcfg_dense/"
cp "$TPL/cfg_dense.xml" "$OUT/bcfg_dense/IKAnalyzer.cfg.xml"

echo "== 装配 gen/probes/acfg（A 侧 classpath 资源目录）=="
mkdir -p "$OUT/acfg"
cp "$TPL/IKAnalyzer.cfg.xml" "$TPL/long_word_dict.dic" "$TPL/test_stopwords.dic" "$OUT/acfg/"

echo "== 装配 gen/probes/acfg_dense（C3：A 侧高密度词典 classpath）=="
mkdir -p "$OUT/acfg_dense"
cp "$TPL/dense.dic" "$OUT/acfg_dense/"
cp "$TPL/cfg_dense.xml" "$OUT/acfg_dense/IKAnalyzer.cfg.xml"

echo "== 校验 C1 一致性 =="
cmp "$A_DIC/main2012.dic" "$OUT/bcfg_unified/main.dic" && echo "  main.dic 与 A 侧逐字节一致"
cmp "$AB_A_SRC/stopword.dic" "$OUT/bcfg_unified/stopword.dic" && echo "  stopword.dic 与 A 侧逐字节一致"
echo "完成。"
