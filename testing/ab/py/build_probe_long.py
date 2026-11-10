#!/usr/bin/env python3
"""生成缓冲区边界阈值扫描探针（probe_long2.txt，14 行）。
实证「输入 >= 4097 字符起，u6 在每个 refill 边界多输出 1 个重叠 token」。
在 gen/ 目录下运行。"""
import io

BASE = "中华人民共和国中央人民政府是一个强大的国家我们正在测试中文分词器的缓冲区边界行为"
L = len(BASE)


def mk(n):
    return (BASE * (n // L + 1))[:n]


LENGTHS = (4095, 4096, 4097, 4098, 4100, 4150, 4196, 4197, 4198, 4200, 4300, 5000, 10000, 20000)
lines = [mk(n) for n in LENGTHS]

with io.open("probe_long2.txt", "w", encoding="utf-8") as f:
    f.write("\n".join(lines) + "\n")
print([len(x) for x in lines])
