#!/usr/bin/env bash
# 跑一轮 A/B 对拍：两侧使用完全相同的词典内容，仅算法/配置不同。
#
# 用法:
#   ./cmp.sh <语料文件> <maxword|smart> [lower|nolower] [B侧词典目录]
#
# 输出: $AB_GEN/.a.txt / $AB_GEN/.b.txt（归一化诊断型类型名后）
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=env.sh
source "$HERE/env.sh"

[ $# -ge 2 ] || { echo "用法: $0 <语料> <maxword|smart> [lower|nolower] [B词典目录]" >&2; exit 2; }
CORPUS=$1; MODE=$2; LOW=${3:-lower}; BD=${4:-$AB_BDICT_UNIFIED}

norm() {
  grep -E '^#' \
  | sed -E 's/\[CN_WORD\]/[CNWORD]/g; s/\[CN_CHAR\]/[CNCHAR]/g; s/\[TYPE_CNUM\]/[CNUM]/g; s/\[TYPE_CQUAN\]/[CQUAN]/g'
}

# 注意：u6 的 Dictionary.loadExtDict/loadStopWordDict 会往 stdout 打
# 「加载扩展词典：…」，必须用 grep '^#' 过滤，否则会整体错位一行。
#
# AB_ACP_EXTRA：可选的 A 侧额外 classpath 前缀（用于 C3 定向探针，
# 必须放在最前，否则 u6 自带 cfg.xml 会抢先被 getResourceAsStream 命中）。
java -cp "${AB_ACP_EXTRA:+$AB_ACP_EXTRA:}$AB_ACP" AProbe "$MODE" < "$CORPUS" 2>/dev/null | norm > "$AB_GEN/.a.txt"
java -cp "$AB_BCP" BProbe "$MODE" "$LOW" "$BD" < "$CORPUS" 2>/dev/null | norm > "$AB_GEN/.b.txt"

echo "已写出: $AB_GEN/.a.txt  $AB_GEN/.b.txt"
echo "差异概览:"
python3 "$HERE/py/tokendiff.py" "$AB_GEN/.a.txt" "$AB_GEN/.b.txt" 2>/dev/null | sed -n '1,3p' || true
