#!/usr/bin/env python3
"""生成代理对跨页扫描探针（probe_split.txt，125 行）。
把单个 😀 依次放在汉字流的第 3985..4104 个 UTF-16 code unit 处，
覆盖 needRefillBuffer 的临界区 cursor ∈ (3996, 4094]。
在 gen/ 目录下运行。"""
import io

E = "\U0001F600"
C = "中"

lines = []
for p in range(3985, 4105):
    lines.append(C * p + E + C * 8)
for p in (4093, 4094, 4095, 4096, 4097):
    lines.append(C * p + E)

with io.open("probe_split.txt", "w", encoding="utf-8") as f:
    for l in lines:
        f.write(l + "\n")
print("lines =", len(lines))
