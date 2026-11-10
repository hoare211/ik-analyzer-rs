#!/usr/bin/env python3
"""生成编码阶段探针：astral 字符 × 4096 缓冲区边界（probe_enc.txt，10 行）。
在 gen/ 目录下运行。"""
import io

E = "\U0001F600"   # 😀 = 2 UTF-16 code units
C = "中"            # 1 code unit

lines = [
    E * 2100,            # 4200 units
    E * 2048,            # 4096 units
    E * 2049,            # 4098
    E * 2048 + C,        # 4097
    C * 4095 + E,        # 4097
    C * 4096 + E,        # 4098 -> 触发 refill
    C * 4094 + E + C * 2,  # 4098
    (C * 99 + E) * 45,          # 每 100 汉字插一个 emoji
    (C * 99 + "e\u0301" + E) * 40,
    ("❤️" + C * 98 + E) * 40,
]

with io.open("probe_enc.txt", "w", encoding="utf-8") as f:
    for l in lines:
        f.write(l + "\n")

for i, l in enumerate(lines):
    print(i, "chars=", len(l), "utf16_units=", len(l.encode("utf-16-le")) // 2)
