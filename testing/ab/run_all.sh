#!/usr/bin/env bash
# 复现两份调研报告中的全部 A 级（实测）实验。
# 产物落在 $AB_GEN，控制台输出即报告各表格的数据来源。
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=env.sh
source "$HERE/env.sh"
cd "$AB_GEN"

# run_cmp <语料> <mode> [B词典目录] [A侧额外classpath前缀]
#   跑对拍并把 .a/.b 快照成 cc_A_<mode>/cc_B_<mode>，
#   供 final_summary.py / analyze_char_corpus.py 等按模式读取。
#   后两个参数用于 C3 定向探针（需替换词典/_cfg）。
run_cmp() {
  local corpus="$1" mode="$2" bdict="${3:-$AB_BDICT_UNIFIED}" acp_extra="${4:-}"
  local base; base="$(basename "$corpus" .txt)"
  AB_ACP_EXTRA="$acp_extra" "$HERE/cmp.sh" "$corpus" "$mode" lower "$bdict" \
      > "$AB_GEN/log_${base}_${mode}.txt" 2>&1 || true
  cp "$AB_GEN/.a.txt" "$AB_GEN/cc_A_${mode}.txt"
  cp "$AB_GEN/.b.txt" "$AB_GEN/cc_B_${mode}.txt"
}

banner() { printf '\n========== %s ==========\n' "$*"; }

# ---------------------------------------------------------------
banner "实验 0：畸形 UTF-8（WTF-8 孤立代理）的解码行为"
# ---------------------------------------------------------------
printf '\xed\xa0\x80' > lone_surrogate.bin
java -cp "$AB_PROBE_CLASSES" DecodeProbe
echo "预期: 含畸形代理字节的 4 字节输入被解码为 3 个 code unit，"
echo "     其中 \\xED\\xA0\\x80 (3 字节) -> 1 个 U+FFFD"
echo "     -> B 的 SurrogatePairSegmenter 孤立代理兜底路径不可达"

# ---------------------------------------------------------------
banner "实验 1：预处理阶段逐字符差分（9500 code unit）"
# ---------------------------------------------------------------
python3 "$HERE/py/build_repertoire.py"
java -cp "$AB_ACP" org.wltea.analyzer.core.CharProbeA   < units.txt > chars_A.txt
java -cp "$AB_BCP" org.wltea.analyzer.core.CharProbeB true  < units.txt > chars_B_low.txt
java -cp "$AB_BCP" org.wltea.analyzer.core.CharProbeB false < units.txt > chars_B_nolow.txt
python3 "$HERE/py/analyze_chars.py"
echo "预期: identifyCharType 差异 = 2048（全部代理区）"
echo "     regularize (low=true)=0, (low=false)=52（26 个 ASCII A-Z）"

# ---------------------------------------------------------------
banner "实验 2：字符系统语料 token 对拍（254 用例 × 2 口径，C1 同词典）"
# ---------------------------------------------------------------
python3 "$HERE/py/build_char_corpus.py"
for M in maxword smart; do
  run_cmp char_corpus.txt "$M"
done
python3 "$HERE/py/final_summary.py"
echo "预期: max_word 相同 195/254；smart 相同 199/254"
echo "     59 例差异 = 代理对 33 + 词典内容 13 + 内置停用词 13"

# ---------------------------------------------------------------
banner "实验 3：代理对跨页扫描（125 位置 × 2 口径）"
# ---------------------------------------------------------------
python3 "$HERE/py/build_probe_split.py"
for M in maxword smart; do run_cmp probe_split.txt "$M"; done
python3 "$HERE/py/analyze_enc.py"
echo "预期: 拆分代理对 A=0 B=0；refill 重叠每口径 A=11 B=0；未覆盖 unit A=0 B=0"

# ---------------------------------------------------------------
banner "实验 4：缓冲区边界阈值（u6 off-by-one）"
# ---------------------------------------------------------------
python3 "$HERE/py/build_probe_long.py"
for M in maxword smart; do
  "$HERE/cmp.sh" probe_long2.txt "$M" || true
  python3 - "$M" <<'PY'
import sys, re
M = sys.argv[1]
def load(p):
    d = {}
    for line in open(p, encoding='utf-8'):
        line = line.rstrip('\n')
        if not line.startswith('#'):
            continue
        i, _, r = line.partition('|')
        d[int(i[1:])] = r.split()
    return d
a, b = load('.a.txt'), load('.b.txt')
res = []
for k in sorted(a):
    res.append((k, '同' if a[k] == b.get(k) else '★'))
first = next((k for k, v in res if v != '同'), None)
print(f"[{M}] 长度4095..20000 共 {len(res)} 行；首个分歧长度档 = {4095+first if first is not None else '无'}")
print(f"      分歧行数 = {sum(1 for _, v in res if v != '同')}  (预期: 输入>=4097 起全部分歧)")
PY
done
echo "预期: 输入 >= 4097 字符起全部分歧（u6 每个 refill 边界多 1 个重叠 token）"

# ---------------------------------------------------------------
banner "实验 5：停用词 vs 零宽字符的因果隔离"
# ---------------------------------------------------------------
run_cmp "$HERE/probes/stopword_vs_zerowidth.txt" maxword
echo "把 'a' 换成 'zz' 后两侧应完全一致（证明差异来自内置停用词，不是零宽字符）："
python3 - <<'PY'
def load(p):
    d = {}
    for line in open(p, encoding='utf-8'):
        line = line.rstrip('\n')
        if not line.startswith('#'):
            continue
        i, _, r = line.partition('|')
        d[int(i[1:])] = r
    return d
a, b = load('.a.txt'), load('.b.txt')
def esc(t): return t.encode('unicode_escape').decode('ascii')
for k in (11, 12):   # zz\u200bzz / 中zz文
    print(f"  [{k}] A: {esc(a.get(k,''))}")
    print(f"        B: {esc(b.get(k,''))}")
PY

# ---------------------------------------------------------------
banner "实验 6：词典侧大小写折叠（C3，ext 词典含 'ABC公司'）"
# ---------------------------------------------------------------
printf 'ABC公司\nabc公司\n我们ABC公司很大\n' > probe_case3.txt
for side in A B; do
  if [ "$side" = A ]; then
    # acfg_case 必须放在 classpath 最前，否则 u6 自带 cfg.xml（ext_dict=ext.dic）抢先命中
    java -cp "$AB_ROOT/probes/acfg_case:$AB_PROBE_CLASSES:$AB_A_CLASSES:$AB_A_SRC" AProbe maxword \
      < probe_case3.txt 2>/dev/null | grep '^#' | sed 's/^/  u6            : /'
  else
    java -cp "$AB_BCP" BProbe maxword lower "$AB_GEN/probes/bcfg_case" \
      < probe_case3.txt 2>/dev/null | grep '^#' | sed 's/^/  B(lower=true) : /'
    java -cp "$AB_BCP" BProbe maxword nolower "$AB_GEN/probes/bcfg_case" \
      < probe_case3.txt 2>/dev/null | grep '^#' | sed 's/^/  B(lower=false): /'
  fi
done
echo "预期: u6 三个输入都命中 abc公司[CNWORD]；B(lower=true) 全部不命中；B(lower=false) 仅 'ABC公司' 命中"

# ---------------------------------------------------------------
banner "实验 7：高密度交叉路径兜底的可达性"
# ---------------------------------------------------------------
echo "  [自造词典] 逐长度扫描（长度 12..44，需 crafted_dense 配置）:"
run_cmp "$HERE/probes/crafted_sweep_12_44.txt" smart \
        "$AB_GEN/probes/bcfg_dense" "$AB_GEN/probes/acfg_dense"
python3 - <<'PY'
def load(p):
    d = {}
    for line in open(p, encoding='utf-8'):
        line = line.rstrip('\n')
        if not line.startswith('#'):
            continue
        i, _, r = line.partition('|')
        d[int(i[1:])] = r
    return d
a, b = load('.a.txt'), load('.b.txt')
n = 0; first = None
for k in sorted(a):
    if a[k] != b.get(k):
        n += 1
        if first is None:
            first = k
print(f"      触发起点 = 长度 {12+first}（预期 15）；触发总数 = {n}")
PY
echo "  [真实词典] 三组对抗构造（长词拼接 / 交叠 / 超长词链，C1 同词典）:"
for f in real_dense_long_words real_overlap_long_words real_long10_chained; do
  run_cmp "$HERE/probes/$f.txt" smart
done
python3 - <<'PY'
def load(p):
    d = {}
    for line in open(p, encoding='utf-8'):
        line = line.rstrip('\n')
        if not line.startswith('#'):
            continue
        i, _, r = line.partition('|')
        d[int(i[1:])] = r
    return d
a, b = load('.a.txt'), load('.b.txt')
n = sum(1 for k in a if a[k] != b.get(k))
print(f"      最后一组：相同 {len(a)-n}/{len(a)}，触发 {n}（预期 0 —— u6 词典下兜底不可达）")
PY

# ---------------------------------------------------------------
banner "完成"
# ---------------------------------------------------------------
echo "全部产物在 $AB_GEN"
echo "对照报告表格核对预期结果即可确认证据链可复现。"
