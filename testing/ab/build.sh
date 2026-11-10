#!/usr/bin/env bash
# 编译 A(u6)、B(analysis-ik) 两侧源码与本工装的 5 个 Java 探针。
# 产物全部落在 $AB_GEN（gitignore 内）。
set -euo pipefail
HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=env.sh
source "$HERE/env.sh"

ab_check || exit 1

echo "== 编译 A（u6，refs 干净原始版）=="
mkdir -p "$AB_A_CLASSES"
find "$AB_A_SRC/org" -name '*.java' > "$AB_GEN/a_srcs.txt"
javac -nowarn -encoding UTF-8 -d "$AB_A_CLASSES" @"$AB_GEN/a_srcs.txt" 2>&1 \
  | grep -v '^注:' || true

echo "== 编译 B（analysis-ik core，全部 .java）=="
mkdir -p "$AB_B_CLASSES"
find "$AB_B_CORE" -name '*.java' > "$AB_GEN/b_srcs.txt"
javac -nowarn -encoding UTF-8 -cp "$AB_B_CP" -d "$AB_B_CLASSES" @"$AB_GEN/b_srcs.txt" 2>&1 \
  | grep -v '^注:' || true

echo "== 编译探针（org.wltea.analyzer.core 包内，访问 package-private CharacterUtil）=="
mkdir -p "$AB_PROBE_CLASSES"
javac -nowarn -encoding UTF-8 -cp "$AB_A_CLASSES:$AB_A_SRC" \
      -d "$AB_PROBE_CLASSES" "$HERE/java/CharProbeA.java" "$HERE/java/AProbe.java" \
      "$HERE/java/DecodeProbe.java" 2>&1 | grep -v '^注:' || true
javac -nowarn -encoding UTF-8 -cp "$AB_B_CP:$AB_B_CLASSES" \
      -d "$AB_PROBE_CLASSES" "$HERE/java/CharProbeB.java" "$HERE/java/BProbe.java" \
      2>&1 | grep -v '^注:' || true

echo "== 完成 =="
printf '  A classes : %s (%d)\n' "$AB_A_CLASSES" "$(find "$AB_A_CLASSES" -name '*.class' | wc -l)"
printf '  B classes : %s (%d)\n' "$AB_B_CLASSES" "$(find "$AB_B_CLASSES" -name '*.class' | wc -l)"
printf '  probes    : %s (%d)\n' "$AB_PROBE_CLASSES" "$(find "$AB_PROBE_CLASSES" -name '*.class' | wc -l)"

# 装配 B 侧词典目录（从两侧源码树，避免把 3MB 词典入库）
"$HERE/assemble_dicts.sh"
