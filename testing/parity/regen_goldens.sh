#!/bin/bash
# shellcheck shell=bash
# 重新生成 testing/parity/ 的 golden 基线（**需要 Java 21 + 本机 analysis-ik 工装**）。
#
# 背景：>4096 字符输入的 parity 基线**不能**用 u6 jar 生成——u6 每页边界重复
# 输出 1 个 token（off-by-one，设计决策 例外清单 #1），
# 必然分歧；也不能用本仓库旧输出自举（失去参照意义）。故基线取自
# **analysis-ik**（`master@6d2d70f`，同样修复了 `cursor+1`；其算法主干与 u6
# 的等同性已由 testing/ab 的 C1 对拍证明：405 行 × 2 口径 34,072 token 仅
# 3 个 token 级差异且全部可解释）。
#
# 语料设计约束（违反会使基线失效，详见 corpus 生成器注释）：
# - 无长单字重复串（触发 B 独有高密度交叉路径兜底，调研 §9.4，本项目不实现）；
# - 无 astral（B 会输出 emoji，与 u6_parity 的 Drop 口径不可调和）；
# - 无「两/几/数」数词（B 的 shouldMatchSingleChar 门控分歧，调研 §9.3）；
# - 无大写字母开头的词典词条（B 词典不折叠致其失效，调研 §5.3）。
#
# 前置条件（与 testing/ab/README.md 相同）：JDK 21+、~/.m2 有 7 个依赖 jar、
# testing/ab 已 build（./build.sh）。
#
# 用法：cd testing/parity && ./regen_goldens.sh
set -euo pipefail

AB_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../ab" && pwd)"
# shellcheck source=../ab/env.sh
source "$AB_ROOT/env.sh"

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CORPUS="$HERE/corpus_long.txt"
PARITY_CFG="$AB_GEN/probes/bcfg_parity"
U6_SRC="${AB_A_SRC}"

echo "[regen] 装配 B 侧 u6-parity 词典目录（u6 分发词典 + 停用词置空）"
mkdir -p "$PARITY_CFG"
cp "$U6_SRC/org/wltea/analyzer/dic/main2012.dic" "$PARITY_CFG/main.dic"
cp "$U6_SRC/org/wltea/analyzer/dic/quantifier.dic" "$PARITY_CFG/quantifier.dic"
cp "$U6_SRC/ext.dic" "$PARITY_CFG/ext.dic"
: > "$PARITY_CFG/stopword.dic"   # 置空：B 默认无条件加载的 32 英文停用词
: > "$PARITY_CFG/surname.dic"    # B 的 6 部词典中这 3 部加载即丢弃，占位即可
: > "$PARITY_CFG/suffix.dic"
: > "$PARITY_CFG/preposition.dic"
cat > "$PARITY_CFG/IKAnalyzer.cfg.xml" <<'XML'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE properties SYSTEM "http://java.sun.com/dtd/properties.dtd">
<properties>
    <comment>u6 parity 口径：u6 分发词典 + 停用词置空（隔离 analysis-ik 算法主干）</comment>
    <entry key="ext_dict">ext.dic;</entry>
    <entry key="ext_stopwords"></entry>
</properties>
XML

# 类型名归一：B 的 getLexemeTypeString() 对 CNUM/CQUAN 返回字面量 "TYPE_CNUM"/
# "TYPE_CQUAN"（拼写与其余类型不一致），统一为 CN_NUM/CN_QUAN 后与 Rust 侧一致。
for mode in smart maxword; do
  echo "[regen] BProbe ${mode} → golden_b_${mode}.txt"
  java -cp "$AB_BCP" BProbe "$mode" lower "$PARITY_CFG" < "$CORPUS" 2>/dev/null \
    | sed -e 's/^#[0-9]*|//' -e 's/TYPE_CNUM/CN_NUM/g' -e 's/TYPE_CQUAN/CN_QUAN/g' \
    > "$HERE/golden_b_${mode}.txt"
done

echo "[regen] 自检：Rust u6_parity 输出应与 golden 逐字节一致"
REPO_ROOT="$(cd "$AB_ROOT/../.." && pwd)"
(cd "$REPO_ROOT" && cargo build --release --example segment_dump)
DUMP="$REPO_ROOT/target/release/examples/segment_dump"
for mode in smart maxword; do
  smart_flag=$([ "$mode" = "smart" ] && echo true || echo false)
  "$DUMP" --u6-parity --smart "$smart_flag" --rich \
    < "$CORPUS" > "/tmp/regen_rust_${mode}.txt"
  if diff -q "$HERE/golden_b_${mode}.txt" "/tmp/regen_rust_${mode}.txt" > /dev/null; then
    echo "[regen] ${mode}: OK（0 分歧）"
  else
    echo "[regen] ${mode}: 与 golden 有分歧（见下）——基线或实现需排查" >&2
    diff "$HERE/golden_b_${mode}.txt" "/tmp/regen_rust_${mode}.txt" | head -5 >&2
    exit 1
  fi
done
echo "[regen] 完成"
