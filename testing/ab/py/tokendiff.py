#!/usr/bin/env python3
"""A/B 输出逐 token 比对：对每一行做集合与序列级差异定位。"""
import sys

def load(p):
    d = {}
    for line in open(p, encoding='utf-8'):
        line = line.rstrip('\n')
        if not line.startswith('#'):
            continue
        idx, _, rest = line.partition('|')
        d[int(idx[1:])] = rest.split()
    return d

a = load(sys.argv[1])
b = load(sys.argv[2])

same = 0
diffs = []
for k in sorted(a):
    if k not in b:
        continue
    if a[k] == b[k]:
        same += 1
    else:
        diffs.append((k, a[k], b[k]))

print(f"相同行数 = {same} / 总行数 {len(a)} ；差异行数 = {len(diffs)}")
print()
# 按差异类型归类
from collections import Counter
kinds = Counter()
for k, ta, tb in diffs:
    sa, sb = set(ta), set(tb)
    only_a = [t for t in ta if t not in sb]
    only_b = [t for t in tb if t not in sa]
    for t in only_a:
        kinds['A-only'] += 1
    for t in only_b:
        kinds['B-only'] += 1
    if not only_a and not only_b:
        kinds['order-only'] += 1
print("差异 token 统计：", dict(kinds))
print()
for k, ta, tb in diffs:
    sa, sb = set(ta), set(tb)
    only_a = [t for t in ta if t not in sb]
    only_b = [t for t in tb if t not in sa]
    print(f"--- 行#{k}")
    print(f"  A: {' '.join(ta)}")
    print(f"  B: {' '.join(tb)}")
    print(f"  A-only: {only_a}")
    print(f"  B-only: {only_b}")
    print()