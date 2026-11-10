#!/usr/bin/env python3
"""紧凑差分：只显示首个分歧点及其上下文，避免超长输出。"""
import sys, re

def load(p):
    d = {}
    for line in open(p, encoding='utf-8'):
        line = line.rstrip('\n')
        if not line.startswith('#'):
            continue
        idx, _, rest = line.partition('|')
        d[int(idx[1:])] = rest.split()
    return d

a, b = load(sys.argv[1]), load(sys.argv[2])
same = 0
for k in sorted(a):
    ta, tb = a[k], b.get(k)
    if ta == tb:
        same += 1
        continue
    print(f"=== 行#{k}  (A:{len(ta)} tok, B:{len(tb)} tok)")
    i = 0
    while i < min(len(ta), len(tb)) and ta[i] == tb[i]:
        i += 1
    lo = max(0, i - 3)
    print(f"  首个分歧在序列第 {i} 个 token")
    print(f"  共同前缀尾部: ...{' '.join(ta[lo:i])}")
    print(f"  A 独有段 : {' '.join(ta[i:i+12])}   ...A末: {' '.join(ta[-2:])}")
    print(f"  B 独有段 : {' '.join(tb[i:i+12])}   ...B末: {' '.join(tb[-2:])}")
    # 偏移是否只是整体平移
    if len(ta) == len(tb):
        deltas = set()
        for x, y in zip(ta, tb):
            if x == y:
                continue
            mx = re.match(r'(.*)\((\d+)-(\d+)\)$', x)
            my = re.match(r'(.*)\((\d+)-(\d+)\)$', y)
            if mx and my:
                deltas.add((int(my.group(2)) - int(mx.group(2)),
                            int(my.group(3)) - int(mx.group(3))))
        if deltas:
            print(f"  偏移平移量集合(B-A): {sorted(deltas)[:6]}")
    print()
print(f"合计：相同 {same} 行 / 共 {len(a)} 行")